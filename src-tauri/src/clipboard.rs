use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use arboard::{Clipboard, ImageData};
use chrono::Utc;
use tauri::{AppHandle, Emitter, Manager};

use crate::categorize::{categorize, hash_content};
use crate::db::{insert_item, open_db, ClipboardItem};

pub const POLL_INTERVAL_MS: u64 = 300;
pub const MAX_ITEMS: i64 = 1000;

/// Shared so `copy_to_clipboard` can suppress re-capturing our own writes.
pub type LastHash = Arc<Mutex<Option<String>>>;

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

fn store_and_emit(
    app: &AppHandle,
    state: &AppState,
    conn: &rusqlite::Connection,
    item: ClipboardItem,
    hash: String,
) {
    if insert_item(conn, &item, MAX_ITEMS).is_ok() {
        *state.last_hash.lock().unwrap() = Some(hash);
        let _ = app.emit("clipboard:new-item", &item);
    }
}

fn capture_and_store(
    clipboard: &mut Clipboard,
    app: &AppHandle,
    state: &AppState,
    conn: &rusqlite::Connection,
) {
    let already_seen = |hash: &str| state.last_hash.lock().unwrap().as_deref() == Some(hash);

    // text first; fall back to image
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
        let mut hashed = Vec::with_capacity(16 + bytes.len());
        hashed.extend_from_slice(&(width as u64).to_le_bytes());
        hashed.extend_from_slice(&(height as u64).to_le_bytes());
        hashed.extend_from_slice(&bytes);
        let hash = hash_content(&hashed);
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
        let mut clipboard = match Clipboard::new() {
            Ok(c) => c,
            Err(e) => {
                eprintln!("clipboard-superpowers: clipboard init failed: {e}");
                return;
            }
        };
        loop {
            capture_and_store(&mut clipboard, &app, &state, &conn);
            thread::sleep(Duration::from_millis(POLL_INTERVAL_MS));
        }
    });
}
