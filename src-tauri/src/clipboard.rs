use std::path::PathBuf;
use std::sync::{Arc, LazyLock, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use arboard::{Clipboard, ImageData};
use chrono::Utc;
use tauri::{AppHandle, Emitter, Manager};

use crate::categorize::{categorize, hash_content};
use crate::db::{insert_item, open_db, ClipboardItem};
use crate::richtext::sanitize_fragment;
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

#[derive(Clone)]
pub struct AppState {
    pub db_path: PathBuf,
    pub images_dir: PathBuf,
    pub settings_path: PathBuf,
    pub settings: Arc<Mutex<Settings>>,
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
            let mut emitted = item;
            emitted.id = outcome.id;
            let _ = app.emit("clipboard:new-item", &emitted);
        }
        Err(e) => {
            eprintln!("clipboard-superpowers: insert failed: {e}");
        }
    }
}

/// Clipboard payload already pulled off the OS clipboard (gate released).
enum Captured {
    Text { text: String, hash: String },
    Image { width: usize, height: usize, bytes: Vec<u8>, hash: String },
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

    // NOTE: text wins when the clipboard holds both text and an image
    // (common when copying images from browsers). Documented v1 tradeoff —
    // image is only stored when get_text() fails.
    let (want_text, want_images) = {
        let s = state.settings.lock().unwrap();
        (s.capture_text, s.capture_images)
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
            return Some(Captured::Text { text, hash });
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
            });
        }
    }
    None
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
        Captured::Text { text, hash } => {
            // HTML flavor is read in a second clipboard open (arboard holds
            // its own handle while reading text — Windows allows one opener).
            // The text is re-read and compared so a mid-read clipboard change
            // can't attach stale formatting to new text.
            let html = read_html_for_text(&text);
            let category = categorize(&text).to_string();
            let item = ClipboardItem {
                id: 0,
                content: text,
                content_hash: hash.clone(),
                category,
                kind: "text".into(),
                pinned: false,
                created_at: Utc::now().to_rfc3339(),
                html,
            };
            store_and_emit(app, state, conn, item, hash);
        }
        Captured::Image { width, height, bytes, hash } => {
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
            };
            store_and_emit(app, state, conn, item, hash);
        }
    }
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
    if formats::Unicode.read_clipboard(&mut text).is_err() || text != expected {
        return None;
    }
    let mut raw: Vec<u8> = Vec::new();
    // Absent flavor is the common case (plain-text copies) — silent too.
    if formats::Html::new()?.read_clipboard(&mut raw).is_err() {
        return None;
    }
    // NOTE: clipboard-win's get_html already slices out the fragment using
    // the header offsets — `raw` is the bare fragment, NOT a full CF_HTML
    // document. Parsing it again always fails (found live: real Chrome
    // bytes rejected). parse_cf_html/build_cf_html remain the write path.
    let fragment = std::str::from_utf8(&raw).ok()?;
    let clean = sanitize_fragment(fragment);
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
}
