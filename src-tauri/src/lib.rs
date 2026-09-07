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
            // Tray-click race: the shell reclaims foreground right after the
            // tray click completes, stealing our just-gained focus — which
            // trips blur-hide and the window flash-hides. Re-assert focus
            // once the click settles; the blur timer cancels itself on gain.
            let reassert = win.clone();
            std::thread::spawn(move || {
                std::thread::sleep(std::time::Duration::from_millis(120));
                if reassert.is_visible().unwrap_or(false)
                    && !reassert.is_focused().unwrap_or(true)
                {
                    let _ = reassert.set_focus();
                }
            });
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
            let t0 = std::time::Instant::now();
            eprintln!("clipboard-superpowers: setup start");
            let (db_path, images_dir) = clipboard::ensure_app_dirs(app.handle());
            let settings_path = app
                .handle()
                .path()
                .app_data_dir()
                .expect("failed to resolve app data dir")
                .join("settings.json");
            let settings = Settings::load(&settings_path);
            eprintln!("clipboard-superpowers: dirs+settings +{:?}", t0.elapsed());
            let state = clipboard::AppState {
                db_path,
                images_dir,
                settings_path,
                settings: Arc::new(Mutex::new(settings.clone())),
                last_hash: Arc::new(Mutex::new(None)),
                deleted: Arc::new(Mutex::new(Vec::new())),
            };
            clipboard::start_polling(app.handle().clone(), state.clone());
            app.manage(state);
            eprintln!("clipboard-superpowers: polling spawned +{:?}", t0.elapsed());

            // Hotkey + autostart + window mode come from settings. Boot-safe:
            // a taken hotkey logs instead of killing startup.
            match apply_hotkey(app.handle(), &settings.hotkey) {
                Ok(()) => eprintln!("clipboard-superpowers: hotkey ok +{:?}", t0.elapsed()),
                Err(e) => eprintln!("clipboard-superpowers: hotkey FAILED ({e}) +{:?}", t0.elapsed()),
            }
            #[cfg(desktop)]
            if let Err(e) = apply_autostart(app.handle(), settings.launch_on_login) {
                eprintln!("clipboard-superpowers: autostart FAILED ({e}) +{:?}", t0.elapsed());
            } else {
                eprintln!("clipboard-superpowers: autostart +{:?}", t0.elapsed());
            }
            if let Err(e) = apply_window_mode(app.handle(), settings.hide_on_blur) {
                eprintln!("clipboard-superpowers: window-mode FAILED ({e}) +{:?}", t0.elapsed());
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
            eprintln!("clipboard-superpowers: tray ready +{:?}", t0.elapsed());

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
