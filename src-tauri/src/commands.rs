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
    // Shared long-lived connection (see DbConn) — no per-command open/init.
    let conn = state.conn.lock().unwrap();
    f(&conn).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_history(state: State<'_, AppState>) -> Result<Vec<ClipboardItem>, String> {
    let limit = state.settings.lock().unwrap().max_items;
    with_conn(&state, |conn| db::get_history_previews(conn, limit))
}

/// Upper bound on rows returned for a *searched* query.
///
/// The empty-query path uses `max_items` (the whole retained history), but a
/// search re-runs on every debounced keystroke while the renderer shows only 50
/// rows at a time — shipping the full history to render 50 means ~1.5 MB of
/// preview payload per keystroke at the 5000-item setting. Paging still reaches
/// the rest via "Show more", so this trades a bound on how deep a search can
/// page for a flat IPC cost.
const SEARCH_RESULT_CAP: i64 = 200;

#[tauri::command]
pub fn search_history(
    state: State<'_, AppState>,
    query: String,
) -> Result<Vec<ClipboardItem>, String> {
    let query = query.trim().to_string();
    let limit = state.settings.lock().unwrap().max_items;
    with_conn(&state, |conn| {
        if query.is_empty() {
            db::get_history_previews(conn, limit)
        } else {
            db::search_history_previews(conn, &query, SEARCH_RESULT_CAP)
        }
    })
}

/// Return the content of ONE retained secret row, for the reveal button.
///
/// The only renderer-facing command that returns secret bytes. Guarded so it
/// cannot be used to bulk-read: it takes a single id, and it refuses
/// non-secret rows (so it can't be used to bypass the 300-char preview cap on
/// ordinary clips). The frontend must not cache the result — it is dropped on
/// window blur and on restart.
#[tauri::command]
pub fn reveal_secret(state: State<'_, AppState>, id: i64) -> Result<String, String> {
    match with_conn(&state, |conn| db::reveal_secret(conn, id))? {
        Some(content) => Ok(content),
        None => Err(format!("no secret item with id {id}")),
    }
}

#[tauri::command]
pub fn delete_item(state: State<'_, AppState>, id: i64) -> Result<(), String> {
    with_conn(&state, |conn| {
        let row: Result<Option<(String, String, String)>, _> = conn
            .query_row(
                "SELECT content, kind, content_hash FROM clipboard_history
                 WHERE id = ?1",
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

/// Full row by id. List/search payloads carry 300-char content previews, so
/// the detail pane loads the complete row here when the selection changes.
#[tauri::command]
pub fn get_history_item(
    state: State<'_, AppState>,
    id: i64,
) -> Result<Option<ClipboardItem>, String> {
    with_conn(&state, |conn| db::get_item_by_id(conn, id))
}

/// Copy text (plus optional sanitized HTML flavor) to the OS clipboard, then
/// bump the matching history row so the card jumps to the top immediately.
/// Shared tail of the by-text and by-id copy commands.
fn copy_text_with_flavors(
    state: &State<'_, AppState>,
    app: &AppHandle,
    text: &str,
    html: Option<&str>,
) -> Result<(), String> {
    let hash = crate::categorize::hash_content(text.as_bytes());
    // Gated + retried: without this a click landing mid-poll-read fails
    // with Windows 1418. Cloned per attempt since retry may run it again.
    write_with_retry(|cb| cb.set_text(text.to_string()))?;
    // HTML second, under the process gate. A failed flavor write still
    // leaves the text flavor in place — degrade to plain, don't fail.
    if let Some(html) = html {
        if let Err(e) = write_html_flavor(&crate::richtext::build_cf_html(html)) {
            eprintln!("clipboard-superpowers: HTML flavor write failed ({e}), text kept");
        }
    }
    // Bump-on-copy is currently disabled (BUMP_ON_COPY, clipboard.rs): cards
    // stay where they are. The implementation is kept — flip the flag to
    // restore jump-to-top on copy.
    if crate::clipboard::BUMP_ON_COPY {
        // Re-copy = most recent: bump the row.
        let conn = state.conn.lock().unwrap();
        let now = chrono::Utc::now().to_rfc3339();
        match db::touch_by_hash(&conn, &hash, &now).map_err(|e| e.to_string())? {
            Some(row) => {
                // Suppress only our own write (time-bound); the bump is done.
                suppress_hash(state, &hash);
                let _ = app.emit("clipboard:new-item", &row);
            }
            // Not in history — leave last_hash alone so the poller inserts it
            // as a fresh row on the next tick.
            None => {}
        }
    }
    // Either way, the clipboard now holds content WE wrote: mark it so the
    // poller never re-captures/re-bumps it (cards must stay in place).
    suppress_hash(state, &hash);
    state.note_copy_write(&hash);
    Ok(())
}

#[tauri::command]
pub fn copy_to_clipboard(
    state: State<'_, AppState>,
    app: AppHandle,
    text: String,
) -> Result<(), String> {
    copy_text_with_flavors(&state, &app, &text, None)
}

/// Copy a history row by id (text/rich/file). The list payload carries only
/// 300-char content previews, so the webview can no longer be trusted to
/// hold full text — the backend loads the complete row itself.
/// Image cards go through `copy_image_to_clipboard` instead.
#[tauri::command]
pub fn copy_history_item(
    state: State<'_, AppState>,
    app: AppHandle,
    id: i64,
) -> Result<(), String> {
    let (text, html) = {
        let conn = state.conn.lock().unwrap();
        match db::get_item_for_copy(&conn, id).map_err(|e| e.to_string())? {
            Some(row) if row.kind != "image" => (row.content, row.html),
            Some(_) => return Err("image cards must use copy_image_to_clipboard".into()),
            None => return Err(format!("no history item with id {id}")),
        }
    };
    copy_text_with_flavors(&state, &app, &text, html.as_deref())
}

/// Copy-then-paste into the previously focused app (PR2), by history id —
/// same as `copy_history_item` but for text rows, then synthesizes Ctrl+V.
///
/// The 150ms sleep blocks this command's worker thread briefly; that is
/// deliberate — the keys must not fire before the OS restores focus.
/// Failures are returned, never swallowed: a failed key-sim means the text
/// is still on the clipboard (copy succeeded), so the user can Ctrl+V by hand.
#[tauri::command]
pub fn paste_history_item(
    state: State<'_, AppState>,
    app: AppHandle,
    id: i64,
) -> Result<(), String> {
    let (text, html) = {
        let conn = state.conn.lock().unwrap();
        match db::get_item_for_copy(&conn, id).map_err(|e| e.to_string())? {
            Some(row) if row.kind == "text" => (row.content, row.html),
            Some(_) => return Err("only text rows support paste".into()),
            None => return Err(format!("no history item with id {id}")),
        }
    };
    copy_text_with_flavors(&state, &app, &text, html.as_deref())?;
    if let Some(win) = app.get_webview_window("main") {
        // Paste auto-hides so focus falls back to the app receiving Ctrl+V.
        if let Err(e) = win.hide() {
            eprintln!("clipboard-superpowers: paste auto-hide failed: {e}");
        }
    }
    std::thread::sleep(std::time::Duration::from_millis(150));
    send_ctrl_v()
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
    if crate::clipboard::BUMP_ON_COPY {
        let conn = state.conn.lock().unwrap();
        let now = chrono::Utc::now().to_rfc3339();
        match db::touch_by_hash(&conn, &hash, &now).map_err(|e| e.to_string())? {
            Some(row) => {
                suppress_hash(&state, &hash);
                let _ = app.emit("clipboard:new-item", &row);
            }
            None => {}
        }
    }
    // Copy-without-jump: mark our own write so the poller skips it.
    suppress_hash(&state, &hash);
    state.note_copy_write(&hash);
    Ok(())
}

/// File metadata for file cards (PR5): total size + what's still on disk.
/// Stat-only — never reads file contents, never follows the open.
#[derive(serde::Serialize)]
pub struct FileMeta {
    /// Sum of FILE sizes only. Directory entries report a stub size (4 KiB
    /// on Windows) that is not content — counting it showed "4 KB" per
    /// folder (seen live). Directories are counted in `dirs` instead;
    /// recursive sizing is deliberately out of scope (unbounded on big trees).
    pub total_bytes: u64,
    pub existing: usize,
    pub dirs: usize,
    pub missing: Vec<String>,
}

#[tauri::command]
pub fn file_meta(paths: Vec<String>) -> FileMeta {
    stat_paths(&paths)
}

fn stat_paths(paths: &[String]) -> FileMeta {
    let mut total_bytes = 0u64;
    let mut existing = 0usize;
    let mut dirs = 0usize;
    let mut missing = Vec::new();
    // Defensive cap mirrors the capture-side truncate.
    for p in paths.iter().take(500) {
        match std::fs::metadata(p) {
            Ok(m) => {
                existing += 1;
                if m.is_dir() {
                    dirs += 1;
                } else {
                    total_bytes += m.len();
                }
            }
            Err(_) => missing.push(p.clone()),
        }
    }
    FileMeta {
        total_bytes,
        existing,
        dirs,
        missing,
    }
}

/// Extract printed text from one of our image cards (PR6). Guarded to the
/// images dir like `read_image_base64`. The text is inserted as a normal
/// history item (categorized + searchable); the source image is untouched
/// and the clipboard is left alone — extraction never destroys state.
#[tauri::command]
pub fn ocr_image(
    state: State<'_, AppState>,
    app: AppHandle,
    path: String,
) -> Result<String, String> {
    let resolved = std::path::Path::new(&path);
    if resolved.parent() != Some(&state.images_dir) {
        return Err("path outside images directory".into());
    }
    let text = crate::ocr::ocr_image_file(&path)?;
    if text.trim().is_empty() {
        return Err("no text found in image".into());
    }
    let secret = crate::categorize::is_secret(&text);
    if secret {
        // Take the policy lock before reading the setting: the check under the
        // lock is the authoritative one, so a secret can never be stored in
        // the window between update_settings saving the setting and purging.
        let _policy = state.secret_policy.lock().unwrap();
        if state.settings.lock().unwrap().skip_secrets {
            return Ok(String::new());
        }
        let (max_items, conn) = {
            let max_items = state.settings.lock().unwrap().max_items;
            let conn = state.conn.lock().unwrap();
            (max_items, conn)
        };
        let item = ClipboardItem {
            id: 0,
            content_hash: crate::categorize::hash_content(text.as_bytes()),
            category: "secret".into(),
            content: text,
            kind: "text".into(),
            pinned: false,
            created_at: chrono::Utc::now().to_rfc3339(),
            html: None,
            source_app: None,
        };
        db::insert_item(&conn, &item, max_items).map_err(|e| e.to_string())?;
        return Ok(String::new());
    }
    let content_hash = crate::categorize::hash_content(text.as_bytes());
    let (max_items, conn) = {
        let max_items = state.settings.lock().unwrap().max_items;
        let conn = state.conn.lock().unwrap();
        (max_items, conn)
    };
    let category = crate::categorize::categorize(&text).to_string();
    let item = ClipboardItem {
        id: 0,
        content_hash: content_hash.clone(),
        category,
        content: text.clone(),
        kind: "text".into(),
        pinned: false,
        created_at: chrono::Utc::now().to_rfc3339(),
        html: None,
        source_app: None,
    };
    db::insert_item(&conn, &item, max_items).map_err(|e| e.to_string())?;
    // Emit and return only the stored, visible row. A duplicate hash can hit
    // a retained secret whose category is deliberately preserved; returning
    // the freshly built OCR text in that case would leak it to the renderer.
    let stored = db::get_by_hash(&conn, &content_hash).map_err(|e| e.to_string())?;
    match stored {
        Some(row) => {
            let _ = app.emit("clipboard:new-item", &row);
            Ok(text)
        }
        None => Ok(String::new()),
    }
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
        assert_eq!(meta.dirs, 0);
        assert_eq!(meta.missing, vec![gone]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn stat_paths_excludes_directory_stub_sizes() {
        // Live: three Explorer folders showed "12 KB" (4 KiB stub each).
        let dir = std::env::temp_dir().join("clipboard-superpowers-pr5-dirs");
        let _ = std::fs::remove_dir_all(&dir);
        let sub = dir.join("sub");
        std::fs::create_dir_all(&sub).unwrap();
        let meta = stat_paths(&[sub.to_string_lossy().to_string()]);
        assert_eq!(meta.existing, 1);
        assert_eq!(meta.dirs, 1);
        assert_eq!(meta.total_bytes, 0, "dir stub size must not count");
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
    parse_hotkey(&settings.actions_hotkey)?;
    let mut s = settings;
    s.max_items = s.max_items.clamp(100, 5000);
    // Persist and publish before the purge so a crash cannot leave the UI
    // claiming a policy the on-disk state does not describe. The policy lock
    // keeps capture from inserting a secret between the purge and the return.
    let purged = {
        let _policy = state.secret_policy.lock().unwrap();
        s.save(&state.settings_path)?;
        *state.settings.lock().unwrap() = s.clone();
        with_conn(&state, |conn| {
            let purged = if s.skip_secrets {
                db::purge_secrets(conn)?
            } else {
                0
            };
            db::prune_old_items(conn, s.max_items)?;
            Ok(purged)
        })?
    };
    // Live side effects happen after the secret state is already consistent.
    crate::apply_hotkey(&app, &s.hotkey)?;
    crate::apply_autostart(&app, s.launch_on_login)?;
    crate::apply_window_mode(&app, s.hide_on_blur)?;
    if purged > 0 {
        // The renderer has no refresh listener today; SettingsPanel's
        // onSaved callback refetches. Keep the event for a future listener
        // without pretending this command currently drives the UI.
        let _ = app.emit("clipboard:refresh", ());
    }
    Ok(s)
}

#[derive(Serialize)]
pub struct HistoryStats {
    pub total: i64,
    pub pinned: i64,
    /// How many rows turning on "skip secrets" would destroy. Surfaced so the
    /// UI can name the number before the user commits to a permanent delete.
    pub secrets: i64,
    pub db_bytes: u64,
}

#[tauri::command]
pub fn get_stats(state: State<'_, AppState>) -> Result<HistoryStats, String> {
    let counts = with_conn(&state, db::get_visible_counts)?;
    let db_bytes = std::fs::metadata(&state.db_path)
        .map(|m| m.len())
        .unwrap_or(0);
    let (total, pinned) = counts;
    let secrets = with_conn(&state, db::count_secrets)?;
    Ok(HistoryStats {
        total,
        pinned,
        secrets,
        db_bytes,
    })
}

/// Delete visible history rows and their orphaned image files. Retained
/// secrets are excluded even when the user clears everything; the skip
/// setting is the explicit purge control for those rows. Returns row count.
#[tauri::command]
pub fn clear_history(state: State<'_, AppState>, delete_pinned: bool) -> Result<i64, String> {
    let (deleted, image_paths) = with_conn(&state, |conn| {
        db::clear_visible_history(conn, delete_pinned)
    })?;
    for path in image_paths {
        let path = std::path::Path::new(&path);
        if path.parent() == Some(&state.images_dir) {
            let _ = std::fs::remove_file(path);
        }
    }
    // The system clipboard still holds the last copy — without suppression
    // the poller re-captures it on the next tick and Clearing never sticks
    // ("cleared 1 item" forever). Seq-guarded, so an explicit re-copy still
    // captures. Best-effort: a failed read just keeps the old behavior.
    suppress_ambient_clipboard(&state);
    Ok(deleted as i64)
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
