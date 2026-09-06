pub mod categorize;
pub mod clipboard;
pub mod commands;
pub mod db;
pub mod settings;

use std::sync::{Arc, Mutex};

use tauri::{
    menu::{Menu, MenuItem},
    Manager, WindowEvent,
};
use tauri_plugin_autostart::MacosLauncher;
use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutState};

use settings::Settings;

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

/// (Re)register the toggle hotkey. Unregisters everything of ours first, so
/// a changed hotkey never leaves the old one behind. Boot-safe: logs instead
/// of failing when another app already holds the combo.
pub fn apply_hotkey(app: &tauri::AppHandle, hotkey: &str) -> Result<(), String> {
    let shortcut = settings::parse_hotkey(hotkey)?;
    let _ = app.global_shortcut().unregister_all();
    app.global_shortcut()
        .on_shortcut(shortcut, |app, _shortcut, event| {
            if event.state() == ShortcutState::Pressed {
                toggle_main_window(app);
            }
        })
        .map_err(|e| e.to_string())
}

pub fn apply_autostart(app: &tauri::AppHandle, enabled: bool) -> Result<(), String> {
    use tauri_plugin_autostart::ManagerExt;
    let launcher = app.autolaunch();
    if enabled {
        launcher.enable().map_err(|e| e.to_string())
    } else {
        launcher.disable().map_err(|e| e.to_string())
    }
}

/// Keep the popup reachable. hide_on_blur off + not-on-top + no taskbar
/// entry = a buried window with no mouse path back, so "don't hide" pins
/// the window on top. Esc / hotkey still dismiss it instantly.
pub fn apply_window_mode(app: &tauri::AppHandle, hide_on_blur: bool) -> Result<(), String> {
    if let Some(win) = app.get_webview_window("main") {
        win.set_always_on_top(!hide_on_blur)
            .map_err(|e| e.to_string())?;
    }
    Ok(())
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
            let settings_path = app
                .handle()
                .path()
                .app_data_dir()
                .expect("failed to resolve app data dir")
                .join("settings.json");
            let settings = Settings::load(&settings_path);
            let state = clipboard::AppState {
                db_path,
                images_dir,
                settings_path,
                settings: Arc::new(Mutex::new(settings.clone())),
                last_hash: Arc::new(Mutex::new(None)),
            };
            clipboard::start_polling(app.handle().clone(), state.clone());
            app.manage(state);

            // Hotkey + autostart + window mode come from settings. Boot-safe:
            // a taken hotkey logs instead of killing startup.
            if let Err(e) = apply_hotkey(app.handle(), &settings.hotkey) {
                eprintln!("clipboard-superpowers: hotkey register failed: {e}");
            }
            #[cfg(desktop)]
            if let Err(e) = apply_autostart(app.handle(), settings.launch_on_login) {
                eprintln!("clipboard-superpowers: autostart failed: {e}");
            }
            if let Err(e) = apply_window_mode(app.handle(), settings.hide_on_blur) {
                eprintln!("clipboard-superpowers: window mode failed: {e}");
            }

            let toggle_label = format!("Show / Hide ({})", settings.hotkey);
            let toggle_item = MenuItem::with_id(
                app,
                "toggle",
                &toggle_label,
                true,
                None::<&str>,
            )?;
            let quit_item = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&toggle_item, &quit_item])?;
            tauri::tray::TrayIconBuilder::with_id("main-tray")
                .icon(app.default_window_icon().expect("window icon").clone())
                .tooltip(format!("Clipboard Superpowers ({})", settings.hotkey).as_str())
                .menu(&menu)
                .show_menu_on_left_click(false)
                .on_menu_event(|app, event| match event.id.as_ref() {
                    "toggle" => toggle_main_window(app),
                    "quit" => app.exit(0),
                    _ => {}
                })
                // Left-click toggles too: with no taskbar entry this is the
                // mouse-only recovery path when the hotkey is forgotten or
                // taken by another app. Right-click still opens the menu.
                .on_tray_icon_event(|tray, event| {
                    if let tauri::tray::TrayIconEvent::Click {
                        button: tauri::tray::MouseButton::Left,
                        ..
                    } = event
                    {
                        toggle_main_window(tray.app_handle());
                    }
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
            commands::get_settings,
            commands::update_settings,
            commands::get_stats,
            commands::clear_history,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
