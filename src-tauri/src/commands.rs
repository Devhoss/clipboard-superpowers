use arboard::ImageData;
use base64::Engine as _;
use rusqlite::OptionalExtension;
use tauri::{AppHandle, Emitter, State};

use crate::clipboard::{suppress_hash, AppState, MAX_ITEMS};
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
pub fn copy_to_clipboard(
    state: State<'_, AppState>,
    app: AppHandle,
    text: String,
) -> Result<(), String> {
    let hash = crate::categorize::hash_content(text.as_bytes());
    let mut cb = arboard::Clipboard::new().map_err(|e| e.to_string())?;
    cb.set_text(text).map_err(|e| e.to_string())?;
    // Re-copy = most recent: bump the row so the card jumps to the top
    // immediately instead of waiting for the next poll tick.
    let conn = db::open_db(&state.db_path.to_string_lossy()).map_err(|e| e.to_string())?;
    let now = chrono::Utc::now().to_rfc3339();
    match db::touch_by_hash(&conn, &hash, &now).map_err(|e| e.to_string())? {
        Some(row) => {
            // Suppress only our own write (time-bound); the bump is done.
            suppress_hash(&state, &hash);
            let _ = app.emit("clipboard:new-item", &row);
        }
        // Not in history — leave last_hash alone so the poller inserts it
        // as a fresh row on the next tick.
        None => {}
    }
    Ok(())
}

#[tauri::command]
pub fn copy_image_to_clipboard(
    state: State<'_, AppState>,
    app: AppHandle,
    path: String,
) -> Result<(), String> {
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
    let conn = db::open_db(&state.db_path.to_string_lossy()).map_err(|e| e.to_string())?;
    let now = chrono::Utc::now().to_rfc3339();
    match db::touch_by_hash(&conn, &hash, &now).map_err(|e| e.to_string())? {
        Some(row) => {
            suppress_hash(&state, &hash);
            let _ = app.emit("clipboard:new-item", &row);
        }
        None => {}
    }
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
