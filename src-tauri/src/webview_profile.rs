//! App-owned WebView2 user-data directory (dead-WebView startup regression).
//!
//! History: with no configured directory, wry defaulted to
//! `%LOCALAPPDATA%\com.hoss.clipsuper` + `EBWebView`. That profile wedged
//! after a Windows restart (unclean shutdown mid-write; no crash, no GPU
//! involvement — renaming the directory was the only fix) and Chromium could
//! not self-recover. This module gives the app a deterministic,
//! app-owned location on E: alongside the other apps, migrates any existing
//! profile into it excluding transient Chromium lock/cache state, keeps the
//! old profile as a backup until the new one is verified alive, and cleans
//! stale lock artifacts on every startup.
//!
//! Mechanics: wry resolves the WebView2 user-data folder as
//! `WEBVIEW2_USER_DATA_FOLDER` (or the builder default) and then appends
//! `EBWebView`. We therefore point the env var at the PARENT
//! (`E:\AppData\ClipboardSuperpowers\WebView2`) and treat
//! `<parent>\EBWebView` as the profile directory. The env var must be set
//! before the first webview controller is created — hence the call at the
//! top of `run()`.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

/// App-owned persistent location. Sits beside the other E:-drive apps
/// (Nemu at `E:\AppData\Nemu`).
const PREFERRED_PARENT: &str = r"E:\AppData\ClipboardSuperpowers\WebView2";
/// Tauri identifier (tauri.conf.json) — the legacy default profile lived at
/// `%LOCALAPPDATA%\<identifier>\EBWebView`.
const LEGACY_IDENTIFIER: &str = "com.hoss.clipsuper";
/// The env-var experiment profile that the app has been running on since the
/// manual workaround; it holds the most recent webview data.
const EXPERIMENT_PARENT: &str = r"E:\clipboard-superpowers-data\EBWebView";

static USING_APP_DIR: AtomicBool = AtomicBool::new(false);

/// True when the app-owned directory was prepared and the webview will (or
/// already does) use it. Only then is finish_migration() allowed to delete
/// pre-migration backups.
pub fn using_app_dir() -> bool {
    USING_APP_DIR.load(Ordering::Relaxed)
}

/// Resolve, migrate, and clean the WebView2 profile. Call ONCE, before any
/// webview can be created. Returns the parent directory to publish as
/// `WEBVIEW2_USER_DATA_FOLDER`, or None to fall back to the wry default
/// (e.g. the preferred drive is unavailable).
pub fn prepare() -> Option<PathBuf> {
    // A user-wide WEBVIEW2_USER_DATA_FOLDER was found in HKCU\Environment
    // pointing at ANOTHER app's directory (E:\wavesurf-data\wv2) — every
    // process inherited it, and Clipboard's WebView2 then tried to initialize
    // against a foreign, possibly-locked user-data dir. That inheritance is
    // the dead-webview regression's root cause, so the app-owned directory
    // always wins over any inherited preset. (The inherited value is logged
    // for diagnosis; removing it from the registry is left to the user.)
    if let Some(preset) = std::env::var_os("WEBVIEW2_USER_DATA_FOLDER") {
        eprintln!(
            "clipboard-superpowers: environment preset WEBVIEW2_USER_DATA_FOLDER={:?} will be OVERRIDDEN by the app-owned directory",
            preset
        );
    }
    let sources = candidate_sources();
    prepare_with_sources(&PathBuf::from(PREFERRED_PARENT), &sources)
}

/// Existing profiles to migrate from, most-recent-first: the legacy default,
/// then the env-var experiment profile the app ran on during the outage.
fn candidate_sources() -> Vec<PathBuf> {
    [
        std::env::var_os("LOCALAPPDATA")
            .map(|l| PathBuf::from(l).join(LEGACY_IDENTIFIER).join("EBWebView")),
        Some(PathBuf::from(EXPERIMENT_PARENT).join("EBWebView")),
    ]
    .into_iter()
    .flatten()
    .filter(|p| p.exists())
    .collect()
}

/// Testable core of [`prepare`]: `sources` are the candidate profiles to
/// migrate (first existing one wins).
fn prepare_with_sources(parent: &Path, sources: &[PathBuf]) -> Option<PathBuf> {
    if let Err(e) = std::fs::create_dir_all(parent) {
        eprintln!(
            "clipboard-superpowers: preferred WebView2 dir unavailable ({}), falling back to default: {e}",
            parent.display()
        );
        return None;
    }
    let new_profile = parent.join("EBWebView");

    if new_profile.exists() {
        // App dir already in use: just make sure no stale process artifacts
        // survived the last shutdown.
        clean_stale_locks(&new_profile);
        USING_APP_DIR.store(true, Ordering::Relaxed);
        return Some(parent.to_path_buf());
    }

    // First run against the app-owned dir: migrate the most recent existing
    // profile, if any. Sources are ordered newest-first.
    for source in sources {
        if !source.exists() {
            continue;
        }
        match migrate_profile(source, &new_profile, parent) {
            Ok(()) => {
                eprintln!(
                    "clipboard-superpowers: migrated WebView2 profile {} -> {}",
                    source.display(),
                    new_profile.display()
                );
            }
            Err(e) => {
                // Migration failed: remove the partial copy and start fresh
                // rather than banking a half-migrated profile. The source is
                // untouched (renamed only after a successful copy).
                eprintln!(
                    "clipboard-superpowers: WebView2 profile migration from {} failed (starting fresh): {e}",
                    source.display()
                );
                let _ = std::fs::remove_dir_all(&new_profile);
            }
        }
        break; // only the first existing source is relevant
    }

    clean_stale_locks(&new_profile);
    USING_APP_DIR.store(true, Ordering::Relaxed);
    Some(parent.to_path_buf())
}

/// Copy `source` profile into `new_profile` (atomic via a temp sibling),
/// excluding transient Chromium lock files and regenerable caches. On
/// success, rename `source` to `<source>.migrated-bak` (kept as a backup
/// until finish_migration removes it after the webview verified alive).
fn migrate_profile(source: &Path, new_profile: &Path, parent: &Path) -> Result<(), String> {
    let staging = parent.join(".profile-migration-tmp");
    let _ = std::fs::remove_dir_all(&staging);
    copy_tree_excluding(source, &staging)
        .map_err(|e| format!("copy failed: {e}"))?;
    std::fs::rename(&staging, new_profile).map_err(|e| format!("activate failed: {e}"))?;
    let backup = source.with_extension("migrated-bak");
    if let Err(e) = std::fs::rename(source, &backup) {
        // Non-fatal: the copy succeeded and the new profile is active; the
        // old directory just stays in place.
        eprintln!(
            "clipboard-superpowers: could not rename old profile to a backup ({}): {e}",
            source.display()
        );
    }
    Ok(())
}

/// Remove stale Chromium/WebView2 process artifacts (LOCK files) from a
/// profile that no browser is currently holding. Best-effort: a live browser
/// holding a lock makes deletion fail, which we ignore. Run on EVERY startup
/// before webview creation — this is what makes stale transient state unable
/// to wedge startup.
fn clean_stale_locks(dir: &Path) {
    clean_stale_locks_inner(dir, &mut 0);
}

fn clean_stale_locks_inner(dir: &Path, removed: &mut usize) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if path.is_dir() {
            clean_stale_locks_inner(&path, removed);
        } else if name.eq_ignore_ascii_case("lockfile") || name.eq_ignore_ascii_case("lock") {
            if std::fs::remove_file(&path).is_ok() {
                *removed += 1;
            }
        }
    }
}

/// Files/directories that must never be migrated: transient process locks
/// (the Nemu root cause) and regenerable caches (slow to copy, corruption-
/// prone mid-write, worthless across resets).
fn is_transient(name: &str, is_dir: bool) -> bool {
    let lower = name.to_ascii_lowercase();
    if !is_dir && (lower == "lockfile" || lower == "lock") {
        return true;
    }
    is_dir
        && matches!(
            lower.as_str(),
            "cache" | "code cache" | "gpucache" | "shadercache" | "grshadercache" | "crashpad"
        )
}

fn copy_tree_excluding(source: &Path, dest: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dest)?;
    for entry in std::fs::read_dir(source)? {
        let entry = entry?;
        let name = entry.file_name();
        let path = entry.path();
        let target = dest.join(&name);
        if path.is_dir() {
            if is_transient(&name.to_string_lossy(), true) {
                continue;
            }
            copy_tree_excluding(&path, &target)?;
        } else {
            if is_transient(&name.to_string_lossy(), false) {
                continue;
            }
            std::fs::copy(&path, &target)?;
        }
    }
    Ok(())
}

/// Called from setup AFTER the main webview window exists — reaching this
/// point proves WebView2 initialized successfully with the app-owned
/// profile, so pre-migration backups can finally be removed. No-op when the
/// app-owned dir isn't in use.
pub fn finish_migration() {
    if !using_app_dir() {
        return;
    }
    // migrate_profile renamed "<source>\EBWebView" to "<source>\EBWebView.migrated-bak".
    let backups = [
        Some(
            PathBuf::from(EXPERIMENT_PARENT)
                .join("EBWebView")
                .with_extension("migrated-bak"),
        ),
        std::env::var_os("LOCALAPPDATA").map(|l| {
            PathBuf::from(l)
                .join(LEGACY_IDENTIFIER)
                .join("EBWebView")
                .with_extension("migrated-bak")
        }),
    ];
    for bak in backups.into_iter().flatten() {
        if bak.exists() {
            match std::fs::remove_dir_all(&bak) {
                Ok(()) => eprintln!(
                    "clipboard-superpowers: removed verified-away profile backup {}",
                    bak.display()
                ),
                Err(e) => eprintln!(
                    "clipboard-superpowers: could not remove backup {}: {e}",
                    bak.display()
                ),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(path: &Path, contents: &[u8]) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, contents).unwrap();
    }

    #[test]
    fn migration_preserves_user_data_and_drops_transient_state() {
        let base = std::env::temp_dir().join(format!("clipsuper-wv2-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let source = base.join("old").join("EBWebView");
        write(&source.join("Local State"), b"{}");
        write(&source.join("Default").join("Preferences"), b"{}");
        write(&source.join("Default").join("Local Storage").join("leveldb").join("000003.log"), b"data");
        // Transient artifacts at several depths: the Nemu root cause.
        write(&source.join("lockfile"), b"stale");
        write(&source.join("Default").join("Local Storage").join("leveldb").join("LOCK"), b"stale");
        write(&source.join("Default").join("Cache").join("f_000001"), b"cache");
        write(&source.join("GPUCache").join("data_0"), b"cache");
        write(&source.join("Crashpad").join("settings.dat"), b"x");

        let parent = base.join("new-parent");
        let new_profile = parent.join("EBWebView");
        migrate_profile(&source, &new_profile, &parent).unwrap();

        // User data preserved.
        assert!(new_profile.join("Local State").exists());
        assert!(new_profile.join("Default").join("Preferences").exists());
        assert!(new_profile
            .join("Default")
            .join("Local Storage")
            .join("leveldb")
            .join("000003.log")
            .exists());
        // Transient state gone — everywhere.
        assert!(!new_profile.join("lockfile").exists());
        assert!(!new_profile
            .join("Default")
            .join("Local Storage")
            .join("leveldb")
            .join("LOCK")
            .exists());
        assert!(!new_profile.join("Default").join("Cache").exists());
        assert!(!new_profile.join("GPUCache").exists());
        assert!(!new_profile.join("Crashpad").exists());
        // Old profile kept as backup.
        assert!(source.with_extension("migrated-bak").join("Local State").exists());
        assert!(!source.exists());
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn clean_stale_locks_removes_locks_but_keeps_data() {
        let base = std::env::temp_dir().join(format!("clipsuper-wv2l-{}", std::process::id()));
        let profile = base.join("EBWebView");
        write(&profile.join("lockfile"), b"stale");
        write(&profile.join("Default").join("Local Storage").join("leveldb").join("LOCK"), b"stale");
        write(&profile.join("Default").join("Preferences"), b"{}");
        clean_stale_locks(&profile);
        assert!(!profile.join("lockfile").exists());
        assert!(!profile
            .join("Default")
            .join("Local Storage")
            .join("leveldb")
            .join("LOCK")
            .exists());
        assert!(profile.join("Default").join("Preferences").exists());
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn prepare_creates_parent_and_skips_existing_profile() {
        let base = std::env::temp_dir().join(format!("clipsuper-wv2p-{}", std::process::id()));
        let parent = base.join("WebView2");
        let got = prepare_with_sources(&parent, &[]).unwrap();
        assert_eq!(got, parent);
        // Second call: parent exists, no migration sources → still fine.
        assert_eq!(prepare_with_sources(&parent, &[]).unwrap(), parent);
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn prepare_migrates_experiment_profile() {
        let base = std::env::temp_dir().join(format!("clipsuper-wv2m-{}", std::process::id()));
        let parent = base.join("WebView2");
        let experiment = base.join("clipboard-superpowers-data").join("EBWebView").join("EBWebView");
        write(&experiment.join("Local State"), b"{}");
        write(&experiment.join("lockfile"), b"stale");
        let got =
            prepare_with_sources(&parent, &[experiment.clone()]).unwrap();
        assert_eq!(got, parent);
        let new_profile = parent.join("EBWebView");
        assert!(new_profile.join("Local State").exists());
        assert!(!new_profile.join("lockfile").exists());
        assert!(experiment.with_extension("migrated-bak").exists());
        let _ = std::fs::remove_dir_all(&base);
    }
}
