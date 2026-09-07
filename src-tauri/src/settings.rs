use serde::{Deserialize, Serialize};
use std::path::Path;
use tauri_plugin_global_shortcut::{Code, Modifiers, Shortcut};

/// Persisted user preferences. Stored as JSON in the app-data dir.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Settings {
    /// e.g. "Ctrl+Alt+V". Parsed on register; validated on save.
    pub hotkey: String,
    /// Cap for unpinned history (100..=5000). Pinned items never pruned.
    pub max_items: i64,
    pub launch_on_login: bool,
    pub capture_text: bool,
    pub capture_images: bool,
    /// Hide the popup when it loses focus. Off = stays until Esc/hotkey.
    pub hide_on_blur: bool,
    /// Skip passwords/OTPs/private keys before they touch the DB (PR3).
    /// Old settings.json files lack this key — the serde default keeps them
    /// loading instead of failing parse and resetting the whole file.
    #[serde(default = "default_skip_secrets")]
    pub skip_secrets: bool,
    /// Capture Explorer file copies (CF_HDROP) as file cards (PR5).
    /// Old settings.json files lack this key — default on, same migration
    /// pattern as skip_secrets.
    #[serde(default = "default_true")]
    pub capture_files: bool,
}

fn default_skip_secrets() -> bool {
    true
}

fn default_true() -> bool {
    true
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            hotkey: "Ctrl+Alt+V".into(),
            max_items: crate::clipboard::MAX_ITEMS,
            launch_on_login: true,
            capture_text: true,
            capture_images: true,
            hide_on_blur: true,
            skip_secrets: true,
            capture_files: true,
        }
    }
}

impl Settings {
    pub fn load(path: &Path) -> Self {
        if let Ok(bytes) = std::fs::read(path) {
            if let Ok(mut s) = serde_json::from_slice::<Settings>(&bytes) {
                s.normalize();
                return s;
            }
        }
        let s = Settings::default();
        let _ = s.save(path);
        s
    }

    pub fn save(&self, path: &Path) -> Result<(), String> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        }
        let json = serde_json::to_string_pretty(self).map_err(|e| e.to_string())?;
        std::fs::write(path, json).map_err(|e| e.to_string())
    }

    /// Clamp ranges so a hand-edited file can't wedge the app.
    fn normalize(&mut self) {
        self.max_items = self.max_items.clamp(100, 5000);
        if parse_hotkey(&self.hotkey).is_err() {
            self.hotkey = Settings::default().hotkey;
        }
    }
}

/// Parse "Ctrl+Alt+V" into a global shortcut. Modifiers: Ctrl, Alt, Shift,
/// Super (Win/Cmd). Keys: A-Z, 0-9, F1-F12.
pub fn parse_hotkey(s: &str) -> Result<Shortcut, String> {
    let mut mods = Modifiers::empty();
    let mut key: Option<&str> = None;
    for part in s.split('+') {
        let p = part.trim();
        match p.to_ascii_lowercase().as_str() {
            "ctrl" | "control" => mods |= Modifiers::CONTROL,
            "alt" => mods |= Modifiers::ALT,
            "shift" => mods |= Modifiers::SHIFT,
            "super" | "win" | "meta" | "cmd" => mods |= Modifiers::SUPER,
            "" => return Err("empty hotkey segment".into()),
            _ => {
                if key.is_some() {
                    return Err(format!("two keys in hotkey: {s}"));
                }
                key = Some(p);
            }
        }
    }
    let k = key.ok_or_else(|| format!("no key in hotkey: {s}"))?;
    if mods.is_empty() {
        return Err("hotkey needs at least one modifier (Ctrl/Alt/Shift/Super)".into());
    }
    let code = key_to_code(k).ok_or_else(|| format!("unsupported key: {k}"))?;
    Ok(Shortcut::new(Some(mods), code))
}

fn key_to_code(k: &str) -> Option<Code> {
    let up = k.to_ascii_uppercase();
    if up.len() == 1 {
        let c = up.chars().next().unwrap();
        if c.is_ascii_alphabetic() {
            return match c {
                'A' => Some(Code::KeyA),
                'B' => Some(Code::KeyB),
                'C' => Some(Code::KeyC),
                'D' => Some(Code::KeyD),
                'E' => Some(Code::KeyE),
                'F' => Some(Code::KeyF),
                'G' => Some(Code::KeyG),
                'H' => Some(Code::KeyH),
                'I' => Some(Code::KeyI),
                'J' => Some(Code::KeyJ),
                'K' => Some(Code::KeyK),
                'L' => Some(Code::KeyL),
                'M' => Some(Code::KeyM),
                'N' => Some(Code::KeyN),
                'O' => Some(Code::KeyO),
                'P' => Some(Code::KeyP),
                'Q' => Some(Code::KeyQ),
                'R' => Some(Code::KeyR),
                'S' => Some(Code::KeyS),
                'T' => Some(Code::KeyT),
                'U' => Some(Code::KeyU),
                'V' => Some(Code::KeyV),
                'W' => Some(Code::KeyW),
                'X' => Some(Code::KeyX),
                'Y' => Some(Code::KeyY),
                'Z' => Some(Code::KeyZ),
                _ => None,
            };
        }
        if c.is_ascii_digit() {
            return match c {
                '0' => Some(Code::Digit0),
                '1' => Some(Code::Digit1),
                '2' => Some(Code::Digit2),
                '3' => Some(Code::Digit3),
                '4' => Some(Code::Digit4),
                '5' => Some(Code::Digit5),
                '6' => Some(Code::Digit6),
                '7' => Some(Code::Digit7),
                '8' => Some(Code::Digit8),
                '9' => Some(Code::Digit9),
                _ => None,
            };
        }
        return None;
    }
    match up.as_str() {
        "F1" => Some(Code::F1),
        "F2" => Some(Code::F2),
        "F3" => Some(Code::F3),
        "F4" => Some(Code::F4),
        "F5" => Some(Code::F5),
        "F6" => Some(Code::F6),
        "F7" => Some(Code::F7),
        "F8" => Some(Code::F8),
        "F9" => Some(Code::F9),
        "F10" => Some(Code::F10),
        "F11" => Some(Code::F11),
        "F12" => Some(Code::F12),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_default_hotkey() {
        assert!(parse_hotkey("Ctrl+Alt+V").is_ok());
    }

    #[test]
    fn rejects_bare_key_and_garbage() {
        assert!(parse_hotkey("V").is_err());
        assert!(parse_hotkey("Ctrl+Alt+Insert").is_err());
        assert!(parse_hotkey("Ctrl+").is_err());
        assert!(parse_hotkey("").is_err());
    }

    #[test]
    fn accepts_digits_function_keys_and_super() {
        assert!(parse_hotkey("Ctrl+Shift+7").is_ok());
        assert!(parse_hotkey("Alt+F9").is_ok());
        assert!(parse_hotkey("Super+V").is_ok());
    }

    #[test]
    fn old_file_without_capture_files_loads_as_true() {
        // Same migration guarantee as skip_secrets: a missing key must not
        // fail the parse and wipe the user's existing preferences.
        let old = r#"{
            "hotkey": "Ctrl+Alt+V",
            "max_items": 1000,
            "launch_on_login": true,
            "capture_text": true,
            "capture_images": true,
            "hide_on_blur": true
        }"#;
        let s: Settings = serde_json::from_str(old).unwrap();
        assert!(s.capture_files);
    }

    #[test]
    fn normalize_repairs_bad_file() {
        let mut s = Settings {
            hotkey: "bogus".into(),
            max_items: 99999,
            ..Settings::default()
        };
        s.normalize();
        assert_eq!(s.hotkey, "Ctrl+Alt+V");
        assert_eq!(s.max_items, 5000);
    }

    #[test]
    fn old_file_without_skip_secrets_loads_as_true() {
        // Pre-PR3 settings.json has no skip_secrets key. It must load with
        // the safe default instead of failing parse (which would wipe the
        // user's hotkey and toggles back to defaults).
        let old = r#"{
            "hotkey": "Ctrl+Shift+V",
            "max_items": 500,
            "launch_on_login": false,
            "capture_text": true,
            "capture_images": true,
            "hide_on_blur": false
        }"#;
        let s: Settings = serde_json::from_str(old).unwrap();
        assert!(s.skip_secrets);
        assert_eq!(s.hotkey, "Ctrl+Shift+V");
    }

    #[test]
    fn explicit_skip_false_survives_round_trip() {
        let mut s = Settings::default();
        s.skip_secrets = false;
        let back: Settings = serde_json::from_str(&serde_json::to_string(&s).unwrap()).unwrap();
        assert!(!back.skip_secrets);
    }
}
