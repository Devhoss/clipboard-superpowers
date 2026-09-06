pub mod categorize;
pub mod clipboard;
pub mod commands;
pub mod db;

use std::sync::{Arc, Mutex};

use tauri::Manager;
use tauri_plugin_global_shortcut::{Code, GlobalShortcutExt, Modifiers, Shortcut, ShortcutState};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .setup(|app| {
            let (db_path, images_dir) = clipboard::ensure_app_dirs(app.handle());
            let state = clipboard::AppState {
                db_path,
                images_dir,
                last_hash: Arc::new(Mutex::new(None)),
            };
            clipboard::start_polling(app.handle().clone(), state.clone());
            app.manage(state);

            #[cfg(desktop)]
            {
                let toggle = Shortcut::new(Some(Modifiers::CONTROL | Modifiers::ALT), Code::KeyV);
                app.global_shortcut().on_shortcut(toggle, |app, _shortcut, event| {
                    if event.state() == ShortcutState::Pressed {
                        if let Some(win) = app.get_webview_window("main") {
                            let visible = win.is_visible().unwrap_or(false);
                            let focused = win.is_focused().unwrap_or(false);
                            if visible && focused {
                                let _ = win.hide();
                            } else {
                                let _ = win.show();
                                let _ = win.center();
                                let _ = win.set_focus();
                            }
                        }
                    }
                })?;
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_history,
            commands::search_history,
            commands::delete_item,
            commands::toggle_pin,
            commands::copy_to_clipboard,
            commands::copy_image_to_clipboard,
            commands::read_image_base64,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
