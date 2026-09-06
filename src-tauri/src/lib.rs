pub mod categorize;
pub mod clipboard;
pub mod commands;
pub mod db;

use std::sync::{Arc, Mutex};

use tauri::{
    menu::{Menu, MenuItem},
    Manager, WindowEvent,
};
use tauri_plugin_autostart::MacosLauncher;
use tauri_plugin_global_shortcut::{Code, GlobalShortcutExt, Modifiers, Shortcut, ShortcutState};

fn toggle_main_window(app: &tauri::AppHandle) {
    if let Some(win) = app.get_webview_window("main") {
        let visible = win.is_visible().unwrap_or(false);
        let focused = win.is_focused().unwrap_or(false);
        if visible && focused {
            let _ = win.hide();
        } else {
            // NOTE: no win.center() here — the window stays where the user
            // dragged it. Initial position comes from tauri.conf.json.
            let _ = win.show();
            let _ = win.set_focus();
        }
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_autostart::init(
            MacosLauncher::LaunchAgent,
            Some(vec![]),
        ))
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
                use tauri_plugin_autostart::ManagerExt;

                let toggle = Shortcut::new(Some(Modifiers::CONTROL | Modifiers::ALT), Code::KeyV);
                app.global_shortcut()
                    .on_shortcut(toggle, |app, _shortcut, event| {
                        if event.state() == ShortcutState::Pressed {
                            toggle_main_window(app);
                        }
                    })?;

                let _ = app.autolaunch().enable();
            }

            let toggle_item = MenuItem::with_id(
                app,
                "toggle",
                "Show / Hide (Ctrl+Alt+V)",
                true,
                None::<&str>,
            )?;
            let quit_item = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&toggle_item, &quit_item])?;
            tauri::tray::TrayIconBuilder::with_id("main-tray")
                .icon(app.default_window_icon().expect("window icon").clone())
                .tooltip("Clipboard Superpowers (Ctrl+Alt+V)")
                .menu(&menu)
                .show_menu_on_left_click(false)
                .on_menu_event(|app, event| match event.id.as_ref() {
                    "toggle" => toggle_main_window(app),
                    "quit" => app.exit(0),
                    _ => {}
                })
                .build(app)?;

            Ok(())
        })
        .on_window_event(|window, event| {
            // the X button hides to tray instead of quitting
            if let WindowEvent::CloseRequested { api, .. } = event {
                let _ = window.hide();
                api.prevent_close();
            }
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
