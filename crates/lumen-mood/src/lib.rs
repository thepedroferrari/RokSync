//! "Describe a feeling" → a lighting plan for named lights, rooms and zones.
//!
//! * `catalog`  – which models/runtime we offer, pinned by SHA-256.
//! * `download` – user-initiated downloads with verification.
//! * `server`   – runs llama.cpp's `llama-server` on 127.0.0.1 and asks it for schema-valid JSON.
//! * `keywords` – keyword-based fallback when no model is installed.
//! * `plan`     – the plan format and how it becomes light states.

pub mod catalog;
pub mod colors;
pub mod download;
pub mod facts;
pub mod keywords;
pub mod plan;
pub mod server;

pub use plan::{Assignment, MoodPlan};

use std::path::PathBuf;

pub const DIR: &str = "RokSync";
pub const LEGACY_DIR: &str = "Lumen";

/// RokSync's folder under `base` (%APPDATA% for settings, %LOCALAPPDATA% for downloads).
/// An existing legacy "Lumen" folder is used as is until [`migrate_app_dir`] has moved it.
pub fn app_dir(base: Option<PathBuf>) -> PathBuf {
    let base = base.unwrap_or_else(|| PathBuf::from("."));
    let new = base.join(DIR);
    let old = base.join(LEGACY_DIR);
    if !new.exists() && old.is_dir() {
        return old;
    }
    new
}

/// Renames a legacy "Lumen" folder under `base` to "RokSync". Does nothing if there's no old
/// folder or the new one already exists. If the rename fails (a file in it is still open), the old
/// folder stays in use.
pub fn migrate_app_dir(base: Option<PathBuf>) -> std::io::Result<bool> {
    let base = base.unwrap_or_else(|| PathBuf::from("."));
    let (new, old) = (base.join(DIR), base.join(LEGACY_DIR));
    if new.exists() || !old.is_dir() {
        return Ok(false);
    }
    std::fs::rename(&old, &new).map(|_| true)
}

/// %LOCALAPPDATA%\RokSync
pub fn data_dir() -> PathBuf {
    app_dir(dirs::data_local_dir())
}

#[cfg(test)]
mod tests {
    use super::{app_dir, migrate_app_dir};

    #[test]
    fn old_folder_is_used_until_moved_then_moved_once() {
        let base = std::env::temp_dir().join(format!("roksync-dir-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(base.join("Lumen")).unwrap();
        std::fs::write(base.join("Lumen").join("config.json"), "{}").unwrap();
        let b = Some(base.clone());

        assert_eq!(app_dir(b.clone()), base.join("Lumen"));
        assert!(migrate_app_dir(b.clone()).unwrap());
        assert_eq!(app_dir(b.clone()), base.join("RokSync"));
        assert_eq!(std::fs::read_to_string(base.join("RokSync").join("config.json")).unwrap(), "{}");
        assert!(!base.join("Lumen").exists());

        // a Lumen folder that reappears after migration is left alone
        std::fs::create_dir_all(base.join("Lumen")).unwrap();
        assert!(!migrate_app_dir(b.clone()).unwrap());
        assert_eq!(app_dir(b), base.join("RokSync"));
        assert!(base.join("Lumen").exists());
        let _ = std::fs::remove_dir_all(base);
    }

    #[test]
    fn fresh_install_uses_the_new_name() {
        let base = std::env::temp_dir().join(format!("roksync-dir-fresh-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        assert!(!migrate_app_dir(Some(base.clone())).unwrap());
        assert_eq!(app_dir(Some(base.clone())), base.join("RokSync"));
        assert!(!base.exists());
    }
}
