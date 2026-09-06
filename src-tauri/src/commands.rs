use arboard::ImageData;
use base64::Engine as _;
use rusqlite::OptionalExtension;
use tauri::State;

use crate::clipboard::{AppState, LastHash, MAX_ITEMS};
use crate::db::{self, ClipboardItem};

fn with_conn<T>(
    state: &State<'_, AppState>,
    f: impl FnOnce(&rusqlite::Connection) -> rusqlite::Result<T>,
) -> Result<T, String> {
    let conn = db::open_db(&state.db_path.to_string_lossy()).map_err(|e| e.to_string())?;
    f(&conn).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_history(state: State<'_, AppState>) -> Result<Vec<ClipboardItem>, String> {
    with_conn(&state, |conn| db::get_all_items(conn, MAX_ITEMS))
}

#[tauri::command]
pub fn search_history(state: State<'_, AppState>, query: String) -> Result<Vec<ClipboardItem>, String> {
    let query = query.trim().to_string();
    with_conn(&state, |conn| {
        if query.is_empty() {
            db::get_all_items(conn, MAX_ITEMS)
        } else {
            db::search_items(conn, &query, 200)
        }
    })
}

#[tauri::command]
pub fn delete_item(state: State<'_, AppState>, id: i64) -> Result<(), String> {
    with_conn(&state, |conn| {
        let row: Result<Option<(String, String)>, _> = conn
            .query_row(
                "SELECT content, kind FROM clipboard_history WHERE id = ?1",
                [id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional();
        match row {
            Ok(Some((content, kind))) => {
                if kind == "image" {
                    let path = std::path::Path::new(&content);
                    if path.parent() == Some(&state.images_dir) {
                        let _ = std::fs::remove_file(path);
                    }
                }
                db::delete_item(conn, id).map(|_| ())
            }
            Ok(None) => Ok(()),
            Err(e) => Err(e),
        }
    })
}

#[tauri::command]
pub fn toggle_pin(state: State<'_, AppState>, id: i64) -> Result<(), String> {
    with_conn(&state, |conn| db::toggle_pin(conn, id))
}

#[tauri::command]
pub fn copy_to_clipboard(state: State<'_, AppState>, text: String) -> Result<(), String> {
    let hash = crate::categorize::hash_content(text.as_bytes());
    let mut cb = arboard::Clipboard::new().map_err(|e| e.to_string())?;
    cb.set_text(text).map_err(|e| e.to_string())?;
    // Only suppress re-capture after the write succeeded — otherwise the
    // next legitimate poll of the same content would be swallowed.
    set_last_hash(&state.last_hash, &hash);
    Ok(())
}

#[tauri::command]
pub fn copy_image_to_clipboard(state: State<'_, AppState>, path: String) -> Result<(), String> {
    let img = image::open(&path).map_err(|e| e.to_string())?.to_rgba8();
    let (width, height) = (img.width() as usize, img.height() as usize);
    // MUST match clipboard::image_hash (dims + pixels), not raw pixels alone.
    let hash = crate::clipboard::image_hash(width, height, img.as_raw());
    let raw = img.into_raw();
    let mut cb = arboard::Clipboard::new().map_err(|e| e.to_string())?;
    cb.set_image(ImageData {
        width,
        height,
        bytes: std::borrow::Cow::Owned(raw),
    })
    .map_err(|e| e.to_string())?;
    set_last_hash(&state.last_hash, &hash);
    Ok(())
}

/// PNG bytes as base64 for rendering thumbnails in the webview.
#[tauri::command]
pub fn read_image_base64(state: State<'_, AppState>, path: String) -> Result<String, String> {
    let resolved = std::path::Path::new(&path);
    if resolved.parent() != Some(&state.images_dir) {
        return Err("path outside images directory".into());
    }
    let bytes = std::fs::read(resolved).map_err(|e| e.to_string())?;
    Ok(base64::engine::general_purpose::STANDARD.encode(bytes))
}

fn set_last_hash(last_hash: &LastHash, hash: &str) {
    *last_hash.lock().unwrap() = Some(hash.to_string());
}
