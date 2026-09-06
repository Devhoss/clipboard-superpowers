use std::path::PathBuf;
use std::sync::{Arc, LazyLock, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use arboard::{Clipboard, ImageData};
use chrono::Utc;
use tauri::{AppHandle, Emitter, Manager};

use crate::categorize::{categorize, hash_content};
use crate::db::{insert_item, open_db, ClipboardItem};

pub const POLL_INTERVAL_MS: u64 = 300;
pub const MAX_ITEMS: i64 = 1000;
/// How long our own clipboard writes are ignored by the poller. Our write
/// lands within a tick or two (300ms); after the window expires an identical
/// external copy counts as a genuine re-copy and bumps to the top.
pub const SUPPRESS_WINDOW_SECS: u64 = 2;

/// Shared so `copy_to_clipboard` can suppress re-capturing our own writes.
/// Time-bound: a permanent hash would swallow legitimate re-copies of the
/// same content forever (copy X twice → second copy ignored).
pub type LastHash = Arc<Mutex<Option<(String, Instant)>>>;

pub fn suppress_hash(state: &AppState, hash: &str) {
    *state.last_hash.lock().unwrap() = Some((hash.to_string(), Instant::now()));
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
    pub last_hash: LastHash,
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
    match insert_item(conn, &item, MAX_ITEMS) {
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

fn capture_and_store(
    app: &AppHandle,
    state: &AppState,
    conn: &rusqlite::Connection,
) {
    // Time-bound suppression: our own writes are ignored for ~2s, but an
    // identical copy after that is a genuine re-copy and must bump to top.
    let already_seen = |hash: &str| seen_recently(state, hash);

    // Hold the gate for the whole read so a click-copy can't slip in
    // mid-read and race us on OpenClipboard.
    let _guard = CLIPBOARD_LOCK.lock().unwrap();
    let mut clipboard = match Clipboard::new() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("clipboard-superpowers: clipboard init failed: {e}");
            return;
        }
    };

    // NOTE: text wins when the clipboard holds both text and an image
    // (common when copying images from browsers). Documented v1 tradeoff —
    // image is only stored when get_text() fails.
    if let Ok(text) = clipboard.get_text() {
        if text.trim().is_empty() {
            return;
        }
        let hash = hash_content(text.as_bytes());
        if already_seen(&hash) {
            return;
        }
        let category = categorize(&text).to_string();
        let item = ClipboardItem {
            id: 0,
            content: text,
            content_hash: hash.clone(),
            category,
            kind: "text".into(),
            pinned: false,
            created_at: Utc::now().to_rfc3339(),
        };
        store_and_emit(app, state, conn, item, hash);
    } else if let Ok(ImageData { width, height, bytes }) = clipboard.get_image() {
        let hash = image_hash(width, height, &bytes);
        if already_seen(&hash) {
            return;
        }
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
        };
        store_and_emit(app, state, conn, item, hash);
    }
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
