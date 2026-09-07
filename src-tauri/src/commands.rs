use arboard::ImageData;
use base64::Engine as _;
use rusqlite::OptionalExtension;
use tauri::{AppHandle, Emitter, State};

use crate::clipboard::{suppress_hash, write_with_retry, AppState};
use crate::db::{self, ClipboardItem};
use crate::settings::{parse_hotkey, Settings};
use serde::Serialize;

fn with_conn<T>(
    state: &State<'_, AppState>,
    f: impl FnOnce(&rusqlite::Connection) -> rusqlite::Result<T>,
) -> Result<T, String> {
    let conn = db::open_db(&state.db_path.to_string_lossy()).map_err(|e| e.to_string())?;
    f(&conn).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_history(state: State<'_, AppState>) -> Result<Vec<ClipboardItem>, String> {
    let limit = state.settings.lock().unwrap().max_items;
    with_conn(&state, |conn| db::get_all_items(conn, limit))
}

#[tauri::command]
pub fn search_history(state: State<'_, AppState>, query: String) -> Result<Vec<ClipboardItem>, String> {
    let query = query.trim().to_string();
    let limit = state.settings.lock().unwrap().max_items;
    with_conn(&state, |conn| {
        if query.is_empty() {
            db::get_all_items(conn, limit)
        } else {
            db::search_items(conn, &query, 200)
        }
    })
}

#[tauri::command]
pub fn delete_item(state: State<'_, AppState>, id: i64) -> Result<(), String> {
    with_conn(&state, |conn| {
        let row: Result<Option<(String, String, String)>, _> = conn
            .query_row(
                "SELECT content, kind, content_hash FROM clipboard_history WHERE id = ?1",
                [id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .optional();
        match row {
            Ok(Some((content, kind, hash))) => {
                if kind == "image" {
                    let path = std::path::Path::new(&content);
                    if path.parent() == Some(&state.images_dir) {
                        let _ = std::fs::remove_file(path);
                    }
                }
                db::delete_item(conn, id).map(|_| ())?;
                // Don't resurrect: the ambient clipboard still holds this
                // content. Seq-guarded, so an explicit re-copy still captures.
                crate::clipboard::note_deleted(&state, &hash);
                Ok(())
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
    // Gated + retried: without this a click landing mid-poll-read fails
    // with Windows 1418. Cloned per attempt since retry may run it again.
    write_with_retry(|cb| cb.set_text(text.clone()))?;
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
    // Same gate+retry as text — image payloads are megabytes, so the race
    // window they hold the clipboard open is widest for them.
    write_with_retry(|cb| {
        cb.set_image(ImageData {
            width,
            height,
            bytes: std::borrow::Cow::Owned(raw.clone()),
        })
    })?;
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

#[tauri::command]
pub fn get_settings(state: State<'_, AppState>) -> Result<Settings, String> {
    Ok(state.settings.lock().unwrap().clone())
}

#[tauri::command]
pub fn update_settings(
    state: State<'_, AppState>,
    app: AppHandle,
    settings: Settings,
) -> Result<Settings, String> {
    // Validate BEFORE persisting anything.
    parse_hotkey(&settings.hotkey)?;
    let mut s = settings;
    s.max_items = s.max_items.clamp(100, 5000);
    s.save(&state.settings_path)?;
    *state.settings.lock().unwrap() = s.clone();
    // Apply live: hotkey re-register, autostart toggle, window mode, prune.
    crate::apply_hotkey(&app, &s.hotkey)?;
    crate::apply_autostart(&app, s.launch_on_login)?;
    crate::apply_window_mode(&app, s.hide_on_blur)?;
    with_conn(&state, |conn| {
        db::prune_old_items(conn, s.max_items)?;
        Ok(())
    })?;
    Ok(s)
}

#[derive(Serialize)]
pub struct HistoryStats {
    pub total: i64,
    pub pinned: i64,
    pub db_bytes: u64,
}

#[tauri::command]
pub fn get_stats(state: State<'_, AppState>) -> Result<HistoryStats, String> {
    with_conn(&state, |conn| {
        let total: i64 =
            conn.query_row("SELECT COUNT(*) FROM clipboard_history", [], |r| r.get(0))?;
        let pinned: i64 = conn.query_row(
            "SELECT COUNT(*) FROM clipboard_history WHERE pinned = 1",
            [],
            |r| r.get(0),
        )?;
        Ok((total, pinned))
    })
    .map(|(total, pinned)| {
        let db_bytes = std::fs::metadata(&state.db_path)
            .map(|m| m.len())
            .unwrap_or(0);
        HistoryStats { total, pinned, db_bytes }
    })
}

/// Delete history rows (and their orphaned image files). Returns row count.
#[tauri::command]
pub fn clear_history(state: State<'_, AppState>, delete_pinned: bool) -> Result<i64, String> {
    with_conn(&state, |conn| {
        let flag = if delete_pinned { 1 } else { 0 };
        let paths: Vec<String> = conn
            .prepare(
                "SELECT content FROM clipboard_history
                 WHERE kind = 'image' AND (?1 = 1 OR pinned = 0)",
            )?
            .query_map([flag], |row| row.get(0))?
            .collect::<Result<Vec<_>, _>>()?;
        let deleted = conn.execute(
            "DELETE FROM clipboard_history WHERE ?1 = 1 OR pinned = 0",
            [flag],
        )? as i64;
        for p in paths {
            let path = std::path::Path::new(&p);
            if path.parent() == Some(&state.images_dir) {
                let _ = std::fs::remove_file(path);
            }
        }
        Ok(deleted)
    })
}
