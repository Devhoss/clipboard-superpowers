use arboard::ImageData;
use base64::Engine as _;
use rusqlite::OptionalExtension;
use tauri::{AppHandle, Emitter, Manager, State};

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

/// Copy text AND its sanitized HTML flavor back to the clipboard (PR4).
/// Word/Notion/Discord read the HTML flavor and keep formatting; Notepad
/// reads the text flavor and degrades gracefully. Same bump + suppress +
/// emit tail as `copy_to_clipboard` (mirrors the `copy_image_to_clipboard`
/// precedent — one shared shape, not a refactor of unrelated commands).
#[tauri::command]
pub fn copy_rich_to_clipboard(
    state: State<'_, AppState>,
    app: AppHandle,
    text: String,
    html: String,
) -> Result<(), String> {
    let hash = crate::categorize::hash_content(text.as_bytes());
    write_with_retry(|cb| cb.set_text(text.clone()))?;
    // HTML second, under the process gate: without it a poll tick can wedge
    // on 1418 mid-write. A failed flavor write still leaves the text flavor
    // in place — degrade to plain, don't fail the whole copy.
    if let Err(e) = write_html_flavor(&crate::richtext::build_cf_html(&html)) {
        eprintln!("clipboard-superpowers: HTML flavor write failed ({e}), text kept");
    }
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

/// Write a full CF_HTML document to the `HTML Format` flavor.
fn write_html_flavor(doc: &str) -> Result<(), String> {
    use clipboard_win::{Clipboard, Setter};
    let _guard = crate::clipboard::CLIPBOARD_LOCK.lock().unwrap();
    let _clip = Clipboard::new_attempts(5).map_err(|e| e.to_string())?;
    clipboard_win::formats::Html::new()
        .ok_or_else(|| "HTML Format not registered".to_string())?
        .write_clipboard(&doc.to_string())
        .map_err(|e| e.to_string())?;
    Ok(())
}

/// Copy-then-paste into the previously focused app (PR2 backend slice).
///
/// Flow: reuse `copy_to_clipboard` (clipboard + bump + suppress + event),
/// hide the popup so focus falls back to the previous app, wait a beat,
/// then synthesize Ctrl+V. Text only — images/HTML stay copy-only.
///
/// The 150ms sleep blocks this command's worker thread briefly; that is
/// deliberate — the keys must not fire before the OS restores focus.
/// Failures are returned, never swallowed: a failed key-sim means the text
/// is still on the clipboard (copy succeeded), so the user can Ctrl+V by hand.
#[tauri::command]
pub fn paste_text_to_previous_app(
    state: State<'_, AppState>,
    app: AppHandle,
    text: String,
) -> Result<(), String> {
    copy_to_clipboard(state, app.clone(), text)?;
    if let Some(win) = app.get_webview_window("main") {
        let _ = win.hide();
    }
    std::thread::sleep(std::time::Duration::from_millis(150));
    send_ctrl_v()
}

/// Raw SendInput Ctrl+V. OS-bound by nature: no automated test fires this
/// (a test that emits real keystrokes into whatever window has focus is
/// worse than no test). Verified manually — see PR body.
fn send_ctrl_v() -> Result<(), String> {
    use enigo::{Direction, Enigo, Key, Keyboard, Settings};
    let mut enigo = Enigo::new(&Settings::default()).map_err(|e| e.to_string())?;
    enigo
        .key(Key::Control, Direction::Press)
        .map_err(|e| e.to_string())?;
    enigo
        .key(Key::Unicode('v'), Direction::Click)
        .map_err(|e| e.to_string())?;
    enigo
        .key(Key::Control, Direction::Release)
        .map_err(|e| e.to_string())?;
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

/// File metadata for file cards (PR5): total size + what's still on disk.
/// Stat-only — never reads file contents, never follows the open.
#[derive(serde::Serialize)]
pub struct FileMeta {
    pub total_bytes: u64,
    pub existing: usize,
    pub missing: Vec<String>,
}

#[tauri::command]
pub fn file_meta(paths: Vec<String>) -> FileMeta {
    stat_paths(&paths)
}

fn stat_paths(paths: &[String]) -> FileMeta {
    let mut total_bytes = 0u64;
    let mut existing = 0usize;
    let mut missing = Vec::new();
    // Defensive cap mirrors the capture-side truncate.
    for p in paths.iter().take(500) {
        match std::fs::metadata(p) {
            Ok(m) => {
                existing += 1;
                total_bytes += m.len();
            }
            Err(_) => missing.push(p.clone()),
        }
    }
    FileMeta { total_bytes, existing, missing }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stat_paths_totals_sizes_and_flags_missing() {
        let dir = std::env::temp_dir().join("clipboard-superpowers-pr5-test");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let a = dir.join("a.txt");
        let b = dir.join("b.bin");
        std::fs::write(&a, vec![0u8; 100]).unwrap();
        std::fs::write(&b, vec![0u8; 200]).unwrap();
        let gone = dir.join("gone.txt").to_string_lossy().to_string();

        let meta = stat_paths(&[
            a.to_string_lossy().to_string(),
            b.to_string_lossy().to_string(),
            gone.clone(),
        ]);
        assert_eq!(meta.total_bytes, 300);
        assert_eq!(meta.existing, 2);
        assert_eq!(meta.missing, vec![gone]);
        let _ = std::fs::remove_dir_all(&dir);
    }
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
    let deleted = with_conn(&state, |conn| {
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
    })?;
    // The system clipboard still holds the last copy — without suppression
    // the poller re-captures it on the next tick and Clearing never sticks
    // ("cleared 1 item" forever). Seq-guarded, so an explicit re-copy still
    // captures. Best-effort: a failed read just keeps the old behavior.
    suppress_ambient_clipboard(&state);
    Ok(deleted)
}

/// Hash whatever sits on the OS clipboard right now into the suppress +
/// deleted registries. Shared by clear_history (delete-all resurrects too).
fn suppress_ambient_clipboard(state: &AppState) {
    use crate::clipboard::{image_hash, note_deleted};
    let _guard = crate::clipboard::CLIPBOARD_LOCK.lock().unwrap();
    let Ok(mut cb) = arboard::Clipboard::new() else {
        return;
    };
    if let Ok(text) = cb.get_text() {
        if !text.trim().is_empty() {
            note_deleted(state, &crate::categorize::hash_content(text.as_bytes()));
        }
        return;
    }
    if let Ok(img) = cb.get_image() {
        note_deleted(state, &image_hash(img.width, img.height, &img.bytes));
    }
}
