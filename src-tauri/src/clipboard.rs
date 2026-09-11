use std::path::PathBuf;
use std::sync::{Arc, LazyLock, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use arboard::{Clipboard, ImageData};
use chrono::Utc;
use tauri::{AppHandle, Emitter, Manager};

use crate::categorize::{categorize, hash_content, is_secret};
use crate::db::{expire_secrets, insert_item, open_db, ClipboardItem};
use crate::richtext::{parse_cf_html, sanitize_fragment};
use crate::settings::Settings;

pub const POLL_INTERVAL_MS: u64 = 300;
pub const MAX_ITEMS: i64 = 1000;
/// Cap for a captured HTML flavor (chars). Full email bodies can be megabytes;
/// beyond this the item stays plain text. Applies after sanitizing.
pub const MAX_HTML_CHARS: usize = 32_000;
/// How long our own clipboard writes are ignored by the poller. Our write
/// lands within a tick or two (300ms); after the window expires an identical
/// external copy counts as a genuine re-copy and bumps to the top.
pub const SUPPRESS_WINDOW_SECS: u64 = 2;
/// How long a captured secret survives when the user turned skipping off
/// (PR3). Fixed — no setting: brief enough to be safe, long enough to paste.
pub const SECRET_TTL_SECS: i64 = 60;

/// Shared so `copy_to_clipboard` can suppress re-capturing our own writes.
/// Time-bound: a permanent hash would swallow legitimate re-copies of the
/// same content forever (copy X twice → second copy ignored).
pub type LastHash = Arc<Mutex<Option<(String, Instant)>>>;

/// Hashes deleted by the user, paired with the clipboard sequence number at
/// delete time. A deleted card must not come back while the SAME clipboard
/// content sits there (ambient re-read every tick), but an explicit re-copy
/// bumps the sequence and is a genuine capture.
pub type DeletedHashes = Arc<Mutex<Vec<(String, u32)>>>;

fn deleted_matches(entries: &[(String, u32)], hash: &str, seq: u32) -> bool {
    entries.iter().any(|(h, s)| h == hash && *s == seq)
}

pub fn suppress_hash(state: &AppState, hash: &str) {
    *state.last_hash.lock().unwrap() = Some((hash.to_string(), Instant::now()));
}

/// Record a user delete so the ambient clipboard content doesn't resurrect
/// the card on the next poll tick. The 2s gate covers the immediate ticks;
/// the (hash, seq) entry covers everything after — until an explicit re-copy
/// bumps the sequence and the content becomes capturable again.
pub fn note_deleted(state: &AppState, hash: &str) {
    suppress_hash(state, hash);
    let Some(seq) = clipboard_win::raw::seq_num() else {
        return;
    };
    let mut d = state.deleted.lock().unwrap();
    d.retain(|(h, _)| h != hash);
    d.push((hash.to_string(), seq.get()));
    while d.len() > 100 {
        d.remove(0);
    }
}

fn is_deleted(state: &AppState, hash: &str) -> bool {
    let d = state.deleted.lock().unwrap();
    if d.is_empty() {
        return false;
    }
    let Some(seq) = clipboard_win::raw::seq_num() else {
        return false;
    };
    deleted_matches(&d, hash, seq.get())
}

fn seen_recently(state: &AppState, hash: &str) -> bool {
    match &*state.last_hash.lock().unwrap() {
        Some((h, t)) => h == hash && t.elapsed().as_secs() < SUPPRESS_WINDOW_SECS,
        None => false,
    }
}

/// Process-wide clipboard gate. Our poller opens the clipboard every 300ms,
/// so a card-click write landing mid-read races on OpenClipboard and Windows
/// answers 1418 ("thread does not have a clipboard open"). Every clipboard
/// access — poll reads and command writes — goes through this lock.
/// External holders (viewers, RDP, screenshot tools) are handled by retry.
pub static CLIPBOARD_LOCK: LazyLock<Mutex<()>> = LazyLock::new(|| Mutex::new(()));

/// Run a clipboard write with the gate held, plus backoff retries for
/// transient external contention. Reads don't need this — the next 300ms
/// tick retries them for free. Must NOT be called while already holding
/// CLIPBOARD_LOCK.
pub fn write_with_retry<T>(
    mut op: impl FnMut(&mut Clipboard) -> Result<T, arboard::Error>,
) -> Result<T, String> {
    let _guard = CLIPBOARD_LOCK.lock().unwrap();
    let mut last = String::new();
    for attempt in 0..5 {
        match Clipboard::new().and_then(|mut cb| op(&mut cb)) {
            Ok(v) => return Ok(v),
            Err(e) => {
                last = e.to_string();
                if attempt < 4 {
                    std::thread::sleep(Duration::from_millis(60 * (attempt as u64 + 1)));
                }
            }
        }
    }
    Err(last)
}

/// One long-lived SQLite connection shared by all Tauri commands. Opening a
/// connection + re-running init_schema cost ~7ms per command in the bench —
/// every delete/pin/copy paid it. The poller keeps its own connection (WAL
/// allows both); writes serialize at SQLite anyway.
pub type DbConn = Arc<Mutex<rusqlite::Connection>>;

#[derive(Clone)]
pub struct AppState {
    pub db_path: PathBuf,
    pub images_dir: PathBuf,
    pub settings_path: PathBuf,
    pub settings: Arc<Mutex<Settings>>,
    pub conn: DbConn,
    pub last_hash: LastHash,
    /// (hash, clipboard-seq-at-delete) pairs. See `deleted_matches`.
    pub deleted: DeletedHashes,
}

fn max_items(state: &AppState) -> i64 {
    state.settings.lock().unwrap().max_items
}

pub fn ensure_app_dirs(app: &AppHandle) -> (PathBuf, PathBuf) {
    let data_dir = app
        .path()
        .app_data_dir()
        .expect("failed to resolve app data dir");
    std::fs::create_dir_all(&data_dir).expect("failed to create app data dir");
    let images_dir = data_dir.join("images");
    std::fs::create_dir_all(&images_dir).expect("failed to create images dir");
    (data_dir.join("clipboard.db"), images_dir)
}

fn save_image_png(dir: &PathBuf, hash: &str, width: usize, height: usize, rgba: &[u8]) -> Option<PathBuf> {
    let img: image::ImageBuffer<image::Rgba<u8>, Vec<u8>> =
        image::ImageBuffer::from_raw(width as u32, height as u32, rgba.to_vec())?;
    let path = dir.join(format!("{hash}.png"));
    img.save_with_format(&path, image::ImageFormat::Png).ok()?;
    Some(path)
}

/// Canonical hash for image clipboard bytes. MUST stay in sync with
/// `copy_image_to_clipboard` in commands.rs — both hash dimensions + pixels
/// so our own writes are recognised and not re-captured.
pub fn image_hash(width: usize, height: usize, bytes: &[u8]) -> String {
    let mut hashed = Vec::with_capacity(16 + bytes.len());
    hashed.extend_from_slice(&(width as u64).to_le_bytes());
    hashed.extend_from_slice(&(height as u64).to_le_bytes());
    hashed.extend_from_slice(bytes);
    hash_content(&hashed)
}

/// Hash for an Explorer file drop. Deliberately NOT the plain text hash of
/// the path list: copying a file card's paths AS TEXT would produce the same
/// bytes, collide on content_hash, and the capture bump would convert the
/// dir entry into a plain text entry (seen live: clicking `src` turned it
/// into a path text card). The path-text copy is its own entry — that is the
/// "two ways to copy a file" model, and the asymmetry with the text path is
/// intentional (file copy-back goes through the text command on purpose).
pub fn file_drop_hash(joined_paths: &str) -> String {
    hash_content(format!("file-drop\0{joined_paths}").as_bytes())
}

/// Best-effort friendly name of the app that wrote the current clipboard
/// contents (the Application row in the detail pane). Uses GetClipboardOwner
/// — the window that last SET the clipboard — so there is no race with the
/// user switching apps before the 300ms poll tick reads the copy. Elevated
/// processes (OpenProcess denied) and ownerless clipboards degrade to None
/// and the UI omits the row. Best-effort by design: never fails a capture.
fn clipboard_owner_app() -> Option<String> {
    use windows::Win32::Foundation::CloseHandle;
    use windows::Win32::System::Threading::{
        OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32,
        PROCESS_QUERY_LIMITED_INFORMATION,
    };
    use windows::Win32::System::DataExchange::GetClipboardOwner;
    use windows::Win32::UI::WindowsAndMessaging::GetWindowThreadProcessId;

    unsafe {
        let hwnd = GetClipboardOwner().ok()?;
        let mut pid = 0u32;
        GetWindowThreadProcessId(hwnd, Some(&mut pid));
        if pid == 0 {
            return None;
        }
        let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
        let mut buf = [0u16; 512];
        let mut len = buf.len() as u32;
        let ok = QueryFullProcessImageNameW(
            handle,
            PROCESS_NAME_WIN32,
            windows::core::PWSTR(buf.as_mut_ptr()),
            &mut len,
        )
        .is_ok();
        let _ = CloseHandle(handle);
        if !ok {
            return None;
        }
        let path = String::from_utf16_lossy(&buf[..len as usize]);
        let exe = std::path::Path::new(&path)
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())?;
        Some(pretty_app_name(&exe))
    }
}

/// Exe stem → friendly name for the detail pane. Unknown apps fall back to
/// the exe name with the first letter capitalized. UWP shells report odd
/// hosts; map the common ones to something honest.
fn pretty_app_name(exe: &str) -> String {
    let known: &str = match exe.to_lowercase().as_str() {
        "code" | "code - insiders" => "VS Code",
        "chrome" => "Google Chrome",
        "msedge" => "Microsoft Edge",
        "firefox" => "Firefox",
        "notepad" => "Notepad",
        "notepad++" => "Notepad++",
        "devenv" => "Visual Studio",
        "windowsterminal" | "wt" => "Windows Terminal",
        "cmd" => "Command Prompt",
        "powershell" | "pwsh" => "PowerShell",
        "explorer" => "File Explorer",
        "snippingtool" | "screenclipping" | "snipaste" => "Snipping Tool",
        "winword" => "Word",
        "excel" => "Excel",
        "powerpnt" => "PowerPoint",
        "outlook" => "Outlook",
        "idea64" | "idea" => "IntelliJ IDEA",
        "pycharm64" => "PyCharm",
        "webstorm64" => "WebStorm",
        "sublime_text" => "Sublime Text",
        "obsidian" => "Obsidian",
        "slack" => "Slack",
        "discord" => "Discord",
        "telegram" => "Telegram",
        "spotify" => "Spotify",
        "figma" => "Figma",
        "applicationframehost" | "runtimebroker" => "Windows app",
        // Services share svchost.exe; "copied by a background service" is the
        // truthful description (seen live: an agent running as a service).
        "svchost" => "Windows service",
        _ => {
            let mut chars = exe.chars();
            return match chars.next() {
                Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
                None => exe.to_string(),
            };
        }
    };
    known.into()
}

fn store_and_emit(
    app: &AppHandle,
    state: &AppState,
    conn: &rusqlite::Connection,
    item: ClipboardItem,
    hash: String,
) {
    match insert_item(conn, &item, max_items(state)) {
        Ok(outcome) => {
            suppress_hash(state, &hash);
            // Emit the STORED row, not the freshly built item: the builder
            // always has pinned=false, so emitting it visually unpinned a
            // card whenever our own copy was re-captured after the suppress
            // window (pin held in DB, UI showed unpinned).
            let emitted = match crate::db::get_by_hash(conn, &hash) {
                Ok(Some(row)) => row,
                _ => {
                    let mut fallback = item;
                    fallback.id = outcome.id;
                    fallback
                }
            };
            let _ = app.emit("clipboard:new-item", &emitted);
        }
        Err(e) => {
            eprintln!("clipboard-superpowers: insert failed: {e}");
        }
    }
}

/// Clipboard payload already pulled off the OS clipboard (gate released).
enum Captured {
    Text {
        text: String,
        hash: String,
        source_app: Option<String>,
    },
    Image {
        width: usize,
        height: usize,
        bytes: Vec<u8>,
        hash: String,
        source_app: Option<String>,
    },
    Files {
        paths: Vec<String>,
        hash: String,
        source_app: Option<String>,
    },
}

/// Read phase: opens the clipboard, copies bytes out, releases the gate.
/// Kept as short as possible — a click-copy waits on this lock, so the slow
/// work (PNG encode, SQLite) happens after it drops.
fn read_clipboard(state: &AppState) -> Option<Captured> {
    let _guard = CLIPBOARD_LOCK.lock().unwrap();
    let mut clipboard = match Clipboard::new() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("clipboard-superpowers: clipboard init failed: {e}");
            return None;
        }
    };

    // Owner of whatever is on the clipboard right now — read once, applied
    // to whichever branch captures. Read inside the gate, right after the
    // handle exists, so owner and content belong to the same write.
    let owner_app = clipboard_owner_app();
    // NOTE: text wins when the clipboard holds both text and an image
    // (common when copying images from browsers). Documented v1 tradeoff —
    // image is only stored when get_text() fails.
    let (want_text, want_images, want_files) = {
        let s = state.settings.lock().unwrap();
        (s.capture_text, s.capture_images, s.capture_files)
    };
    if want_text {
        if let Ok(text) = clipboard.get_text() {
            if text.trim().is_empty() {
                return None;
            }
            let hash = hash_content(text.as_bytes());
            // Time-bound suppression: our own writes are ignored for ~2s, but
            // an identical copy after that is a genuine re-copy and must bump.
            if seen_recently(state, &hash) {
                return None;
            }
            // User-deleted: ambient re-reads stay dead until a real re-copy.
            if is_deleted(state, &hash) {
                return None;
            }
            return Some(Captured::Text {
                text,
                hash,
                source_app: owner_app,
            });
        }
    }
    if want_images {
        if let Ok(ImageData { width, height, bytes }) = clipboard.get_image() {
            let hash = image_hash(width, height, &bytes);
            if seen_recently(state, &hash) {
                return None;
            }
            if is_deleted(state, &hash) {
                return None;
            }
            return Some(Captured::Image {
                width,
                height,
                bytes: bytes.into_owned(),
                hash,
                source_app: owner_app,
            });
        }
    }
    // Files last, with the arboard handle AND the process gate dropped:
    // clipboard-win opens the clipboard itself, Windows allows exactly one
    // opener, and std Mutex is not reentrant (re-locking would deadlock).
    // The gate is re-taken inside (new_attempts retries a 1418).
    drop(clipboard);
    drop(_guard);
    if want_files {
        if let Some(paths) = read_file_list() {
            let joined = paths.join("\n");
            let hash = file_drop_hash(&joined);
            if seen_recently(state, &hash) {
                return None;
            }
            return Some(Captured::Files {
                paths,
                hash,
                source_app: owner_app,
            });
        }
    }
    None
}

/// Read CF_HDROP via clipboard-win (PR5). Takes CLIPBOARD_LOCK itself —
/// call only after arboard's handle is dropped (see above).
fn read_file_list() -> Option<Vec<String>> {
    use clipboard_win::{formats, Clipboard, Getter};
    let _guard = CLIPBOARD_LOCK.lock().unwrap();
    let _clip = Clipboard::new_attempts(3).ok()?;
    let mut paths: Vec<String> = Vec::new();
    formats::FileList.read_clipboard(&mut paths).ok()?;
    if paths.is_empty() {
        return None;
    }
    // A 10k-file multi-select would store megabytes of paths. 500 keeps
    // real selections intact while bounding the row.
    paths.truncate(500);
    Some(paths)
}

fn capture_and_store(
    app: &AppHandle,
    state: &AppState,
    conn: &rusqlite::Connection,
) {
    // Store phase runs WITHOUT the gate: bytes are already in memory, so
    // PNG encoding + SQLite never block a click-copy.
    let Some(captured) = read_clipboard(state) else {
        return;
    };
    match captured {
        Captured::Text { text, hash, source_app } => {
            // Secret sweep runs every tick (PR3): one cheap DELETE keeps the
            // 60s TTL honest even when nothing new is captured.
            let cutoff = (Utc::now() - chrono::Duration::seconds(SECRET_TTL_SECS))
                .to_rfc3339();
            if let Err(e) = expire_secrets(conn, &cutoff) {
                eprintln!("clipboard-superpowers: secret expiry failed: {e}");
            }
            if is_secret(&text) {
                if state.settings.lock().unwrap().skip_secrets {
                    // Drop silently but suppress the hash — otherwise the
                    // poller re-reads and re-drops the same secret every tick
                    // after the 2s window expires.
                    suppress_hash(state, &hash);
                    return;
                }
                // Captured with the secret category: visible for pasting,
                // gone after SECRET_TTL_SECS via the sweep above.
                let item = ClipboardItem {
                    id: 0,
                    content: text,
                    content_hash: hash.clone(),
                    category: "secret".into(),
                    kind: "text".into(),
                    pinned: false,
                    created_at: Utc::now().to_rfc3339(),
                    html: None,
                    source_app: None,
                };
                store_and_emit(app, state, conn, item, hash);
                return;
            }
            // HTML flavor is read in a second clipboard open (arboard holds
            // its own handle while reading text — Windows allows one opener).
            // The text is re-read and compared so a mid-read clipboard change
            // can't attach stale formatting to new text.
            let html = read_html_for_text(&text);
            let category = code_aware_category(&categorize(&text), &html);
            let item = ClipboardItem {
                id: 0,
                content: text,
                content_hash: hash.clone(),
                category,
                kind: "text".into(),
                pinned: false,
                created_at: Utc::now().to_rfc3339(),
                html,
                source_app,
            };
            store_and_emit(app, state, conn, item, hash);
        }
        Captured::Image { width, height, bytes, hash, source_app } => {
            let Some(path) = save_image_png(&state.images_dir, &hash, width, height, &bytes) else {
                return;
            };
            let item = ClipboardItem {
                id: 0,
                content: path.to_string_lossy().to_string(),
                content_hash: hash.clone(),
                category: "image".into(),
                kind: "image".into(),
                pinned: false,
                created_at: Utc::now().to_rfc3339(),
                html: None,
                source_app,
            };
            store_and_emit(app, state, conn, item, hash);
        }
        Captured::Files { paths, hash, source_app } => {
            // content is the plain path list: clicking the card copies paths
            // as text through the existing copy path (same hash → bump, no
            // duplicate row). Sizes come lazily via file_meta (PR5).
            let item = ClipboardItem {
                id: 0,
                content: paths.join("\n"),
                content_hash: hash.clone(),
                category: "file".into(),
                kind: "file".into(),
                pinned: false,
                created_at: Utc::now().to_rfc3339(),
                html: None,
                source_app,
            };
            store_and_emit(app, state, conn, item, hash);
        }
    }
}

/// IDE HTML flavor outranks plain-text heuristics: a clip that `categorize`
/// called plain but whose sanitized HTML is clearly syntax-highlighted code
/// (`<pre>` / monospace, see `richtext::looks_like_code_html`) belongs in the
/// Code tab. Only plain is overridden — the other categories are single-line
/// or structural convictions that never coincide with a monospace flavor.
fn code_aware_category(text_category: &str, html: &Option<String>) -> String {
    if text_category == "plain"
        && html
            .as_deref()
            .is_some_and(crate::richtext::looks_like_code_html)
    {
        return "code".into();
    }
    text_category.to_string()
}

/// Get the usable fragment out of whatever `formats::Html` handed us.
/// Pure (no clipboard access) so it can be unit-tested against captured
/// real-world bytes. See `read_html_for_text` for why both shapes exist.
///
/// Rejects degenerate inputs instead of banking them:
/// - full documents with stale offsets (header kept as text otherwise),
/// - fragments starting mid-attribute (`e: ; --tw-...;">x</li></ul>` — the
///   StartFragment offset can land inside a tag; the dangling attribute text
///   would render as visible CSS garbage),
/// - tag-free text (no formatting to keep — stays a plain card).
/// A rejected capture stores NULL, so a re-copy can only flip plain→rich,
/// never rich→garbage→rich.
fn extract_fragment(raw: &[u8]) -> Option<String> {
    if let Some(frag) = parse_cf_html(raw) {
        return first_element_from(&frag);
    }
    // Not a parseable document: could be a bare fragment, or a document with
    // stale offsets whose header would otherwise leak in as text (seen live:
    // "Version:0.9 StartHTML:..." stored in the DB). Drop header lines.
    let text = std::str::from_utf8(raw).ok()?;
    let body = match text.find('<') {
        Some(0) => text,
        Some(i) => &text[i..],
        None => return None,
    };
    // Header lines end at the first tag; but a bare mid-attribute slice has
    // no header — first_element_from still rejects it below.
    first_element_from(body)
}

/// Trim to the first real element. Returns None when there is no opening
/// formatting tag — closing tags alone (`</li></ul>`) or bare text carry no
/// recoverable formatting.
fn first_element_from(frag: &str) -> Option<String> {
    let start = frag.find('<')?;
    let frag = &frag[start..];
    const TAGS: [&str; 19] = [
        "h1", "h2", "h3", "h4", "h5", "h6", "p", "div", "span", "b", "strong",
        "i", "em", "u", "a", "ul", "ol", "li", "pre",
    ];
    let lower = frag.to_lowercase();
    let ok = TAGS.iter().any(|t| {
        lower.contains(&format!("<{t}>"))
            || lower.contains(&format!("<{t} "))
    });
    ok.then(|| frag.to_string())
}

/// Read the `HTML Format` flavor for `expected` text (PR4).
/// Returns the sanitized fragment, or None when the clipboard holds no HTML,
/// the text moved on mid-read, or the fragment is empty/oversized.
/// Plain-text copies are unaffected — this only ever ADDS formatting.
fn read_html_for_text(expected: &str) -> Option<String> {
    use clipboard_win::{formats, Clipboard, Getter};
    let _guard = CLIPBOARD_LOCK.lock().unwrap();
    let _clip = Clipboard::new_attempts(3).ok()?;
    let mut text = String::new();
    // Re-read and compare: a mid-read clipboard change must not attach
    // stale formatting to new text. Mismatch is a normal race — silent.
    if formats::Unicode.read_clipboard(&mut text).is_err() || text.trim() != expected.trim() {
        return None;
    }
    let mut raw: Vec<u8> = Vec::new();
    // Absent flavor is the common case (plain-text copies) — silent too.
    if formats::Html::new()?.read_clipboard(&mut raw).is_err() {
        return None;
    }
    // clipboard-win is inconsistent here (proven live): sometimes `raw` is a
    // full CF_HTML document with the Version:/StartFragment: header, sometimes
    // the already-sliced bare fragment. Try the document parse first — it
    // validates offsets — and fall back to the raw bytes as a fragment.
    // Parsing unconditionally broke real Chrome copies; storing unconditionally
    // banks header garbage (seen live: "Version:0.9 StartHTML:..." in the DB).
    let fragment = extract_fragment(&raw)?;
    let clean = sanitize_fragment(&fragment);
    if clean.trim().is_empty() || clean.len() > MAX_HTML_CHARS {
        return None;
    }
    Some(clean)
}

pub fn start_polling(app: AppHandle, state: AppState) {
    thread::spawn(move || {
        let db_path = state.db_path.to_string_lossy().to_string();
        let conn = match open_db(&db_path) {
            Ok(c) => c,
            Err(e) => {
                eprintln!("clipboard-superpowers: db open failed: {e}");
                return;
            }
        };
        // NOTE: Clipboard handle is created fresh each tick under the
        // process-wide gate (see CLIPBOARD_LOCK). Reusing one handle across
        // the loop can hold the Windows clipboard open and wedge after lock
        // contention — per-tick creation is cheap.
        loop {
            capture_and_store(&app, &state, &conn);
            thread::sleep(Duration::from_millis(POLL_INTERVAL_MS));
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ide_html_flavor_promotes_plain_to_code() {
        let mono = Some("<div style=\"font-family: Consolas, monospace\">x = 1</div>".to_string());
        assert_eq!(code_aware_category("plain", &mono), "code");
        assert_eq!(code_aware_category("plain", &None), "plain");
        assert_eq!(code_aware_category("code", &None), "code");
        // Other convictions are never overridden by the flavor.
        assert_eq!(code_aware_category("link", &mono), "link");
        assert_eq!(code_aware_category("color", &mono), "color");
    }

    #[test]
    fn file_drop_hash_differs_from_text_hash() {
        let joined = "E:\\dev\\wavesurf\\src";
        // The whole point of the tag: a dir drop and its path copied as text
        // must be separate entries, never collide on content_hash.
        assert_ne!(file_drop_hash(joined), hash_content(joined.as_bytes()));
        assert_eq!(file_drop_hash(joined), file_drop_hash(joined));
        assert_ne!(file_drop_hash("a\nb"), file_drop_hash("a"));
    }

    #[test]
    fn deleted_matches_same_hash_and_seq_only() {
        let entries = vec![("abc".to_string(), 7u32)];
        assert!(deleted_matches(&entries, "abc", 7));
        // New copy bumps the sequence: capturable again.
        assert!(!deleted_matches(&entries, "abc", 8));
        // Different content never matches.
        assert!(!deleted_matches(&entries, "xyz", 7));
        // Empty registry matches nothing.
        assert!(!deleted_matches(&[], "abc", 7));
    }

    #[test]
    fn extract_fragment_handles_full_document_and_bare_fragment() {
        use crate::richtext::build_cf_html;
        // Shape 1 (seen live): full CF_HTML document with header.
        let doc = build_cf_html("<b>hi</b>");
        assert_eq!(extract_fragment(doc.as_bytes()).as_deref(), Some("<b>hi</b>"));
        // Shape 2 (seen live): already-sliced bare fragment, no header.
        assert_eq!(
            extract_fragment(b"<h1>x</h1>").as_deref(),
            Some("<h1>x</h1>")
        );
        // Garbage in: None (caller stores nothing).
        assert_eq!(extract_fragment(b"\xff\xfe binary").as_deref(), None);
    }

    #[test]
    fn extract_fragment_rejects_degenerate_shapes() {
        // Live: StartFragment offset landed mid-attribute — dangling CSS
        // would otherwise render as visible garbage text.
        let mid_tag = b"e: ; --tw-numeric-spacing: ; font-size: 1.25rem;\">French bulldog</li></ul>";
        assert_eq!(extract_fragment(mid_tag).as_deref(), None);
        // Closing tags alone carry no recoverable formatting.
        assert_eq!(extract_fragment(b"</li></ul>").as_deref(), None);
        // Tag-free text is not HTML — stays a plain card.
        assert_eq!(extract_fragment(b"just text").as_deref(), None);
        // Live (link card): full document with stale offsets — header must
        // not leak in as text, anchor must survive.
        let stale = b"Version:0.9
\nStartHTML:00000097
\nEndHTML:00000295
\nStartFragment:00000131
\nEndFragment:00000259
\n
\n<a href=\"https://x.com\">x</a>";
        let got = extract_fragment(stale).unwrap();
        assert!(!got.contains("Version"), "header leaked: {got:?}");
        assert!(got.contains("<a href=\"https://x.com\">"), "anchor lost: {got:?}");
    }
}
