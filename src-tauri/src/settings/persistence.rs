//! Atomic JSON persistence for [`Settings`] in the platform app-config directory
//! (`%APPDATA%\com.keyclick.app` on Windows,
//! `~/Library/Application Support/com.keyclick.app` on macOS).

use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use super::Settings;

const FILE_NAME: &str = "settings.json";

pub struct SettingsStore {
    path: PathBuf,
}

impl SettingsStore {
    pub fn new(config_dir: &Path) -> Self {
        Self { path: config_dir.join(FILE_NAME) }
    }

    #[cfg(test)]
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Loads settings. A missing file yields defaults; a corrupt file is moved
    /// aside to `settings.json.bak` and defaults are used.
    pub fn load(&self) -> Settings {
        match fs::read(&self.path) {
            Ok(bytes) => match serde_json::from_slice::<Settings>(&bytes) {
                Ok(s) => s.sanitized(),
                Err(_) => {
                    let _ = fs::rename(&self.path, self.path.with_extension("json.bak"));
                    Settings::default()
                }
            },
            Err(_) => Settings::default(),
        }
    }

    /// Writes to a temp file and renames it over the target so a crash can
    /// never leave a half-written settings file.
    pub fn save(&self, settings: &Settings) -> io::Result<()> {
        if let Some(dir) = self.path.parent() {
            fs::create_dir_all(dir)?;
        }
        let tmp = self.path.with_extension("json.tmp");
        {
            let mut f = fs::File::create(&tmp)?;
            f.write_all(&serde_json::to_vec_pretty(settings).map_err(io::Error::other)?)?;
            f.sync_all()?;
        }
        fs::rename(&tmp, &self.path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_file_gives_defaults() {
        let dir = tempfile::tempdir().unwrap();
        let store = SettingsStore::new(dir.path());
        assert_eq!(store.load(), Settings::default());
    }

    #[test]
    fn roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let store = SettingsStore::new(&dir.path().join("nested"));
        let s = Settings {
            enabled: false,
            volume: 0.75,
            sound_pack: "thock".into(),
            modifier_sound: true,
            onboarding_complete: true,
            ..Default::default()
        };
        store.save(&s).unwrap();
        assert_eq!(store.load(), s);
        // Overwrite works (rename over existing file).
        let s2 = Settings { volume: 0.1, ..s };
        store.save(&s2).unwrap();
        assert_eq!(store.load(), s2);
        assert!(!store.path().with_extension("json.tmp").exists());
    }

    #[test]
    fn corrupt_file_is_backed_up_and_defaults_used() {
        let dir = tempfile::tempdir().unwrap();
        let store = SettingsStore::new(dir.path());
        fs::write(store.path(), b"{ not json").unwrap();
        assert_eq!(store.load(), Settings::default());
        assert!(dir.path().join("settings.json.bak").exists());
    }

    #[test]
    fn out_of_range_values_are_sanitized_on_load() {
        let dir = tempfile::tempdir().unwrap();
        let store = SettingsStore::new(dir.path());
        fs::write(store.path(), br#"{ "volume": 7.0, "sound_pack": "../x" }"#).unwrap();
        let s = store.load();
        assert_eq!(s.volume, 1.0);
        assert_eq!(s.sound_pack, "mechanical");
    }
}
