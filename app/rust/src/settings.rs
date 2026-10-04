//! Portable preferences beside the executable; unknown JSON keys survive a save.

use crate::win::{self, Error, hash::io_error};
use serde_json::{Map, Value};
use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

/// The settings document; absent or invalid preferences use their defaults.
#[derive(Default)]
pub struct Settings {
    values: Map<String, Value>,
}

impl Settings {
    /// Loads portable settings, recording read/parse errors and falling back to defaults.
    pub fn load() -> Self {
        let loaded = settings_path().and_then(|path| Self::read(&path));
        loaded.unwrap_or_else(|error| {
            win::record(error);
            Self::default()
        })
    }

    fn read(path: &Path) -> win::Result<Self> {
        let bytes = fs::read(path).map_err(|e| io_error("Read settings", e))?;
        let values: Map<String, Value> = serde_json::from_slice(&bytes)
            .map_err(|e| Error::msg("Read settings JSON", e.to_string()))?;
        if values
            .get("check_updates_on_start")
            .is_some_and(|v| !v.is_boolean())
        {
            return Err(Error::msg(
                "Read settings JSON",
                "check_updates_on_start must be a boolean",
            ));
        }
        Ok(Self { values })
    }

    /// Whether a future startup should check for updates; defaults to false.
    pub fn check_updates_on_start(&self) -> bool {
        self.values
            .get("check_updates_on_start")
            .and_then(Value::as_bool)
            .unwrap_or(false)
    }

    /// Saves the preference at once, retaining the old in-memory value on failure.
    pub fn set_check_updates_on_start(&mut self, checked: bool) -> win::Result<()> {
        self.save_to(&settings_path()?, checked)
    }

    fn save_to(&mut self, path: &Path, checked: bool) -> win::Result<()> {
        let mut values = self.values.clone();
        values.insert("check_updates_on_start".to_owned(), Value::Bool(checked));
        let json = serde_json::to_string_pretty(&values)
            .map_err(|e| Error::msg("Serialize settings", e.to_string()))?
            .replace('\n', "\r\n");
        atomic_write(path, json.as_bytes())?;
        self.values = values;
        Ok(())
    }
}

fn settings_path() -> win::Result<PathBuf> {
    let exe = std::env::current_exe().map_err(|e| io_error("Locate settings folder", e))?;
    let folder = exe
        .parent()
        .ok_or_else(|| Error::msg("Locate settings folder", "no parent folder"))?;
    Ok(folder.join("HWIDChecker.settings.json"))
}

struct PendingFile {
    path: PathBuf,
    file: Option<File>,
    published: bool,
}

impl Drop for PendingFile {
    fn drop(&mut self) {
        drop(self.file.take());
        if !self.published
            && let Err(error) = fs::remove_file(&self.path)
        {
            win::record(io_error("Remove temporary settings", error));
        }
    }
}

// The whitelist helper is private in a separately owned module. Keep its create-new,
// flush, close, rename and cleanup sequence here until that helper can be shared.
fn atomic_write(path: &Path, bytes: &[u8]) -> win::Result<()> {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let parent = path
        .parent()
        .ok_or_else(|| Error::msg("Write settings", "missing parent folder"))?;
    for _ in 0..16 {
        let temp = parent.join(format!(
            ".HWIDChecker.settings.{}.{}.tmp",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let file = match OpenOptions::new().write(true).create_new(true).open(&temp) {
            Ok(file) => file,
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(io_error("Create temporary settings", error)),
        };
        let mut pending = PendingFile {
            path: temp,
            file: Some(file),
            published: false,
        };
        if let Some(file) = &mut pending.file {
            file.write_all(bytes)
                .map_err(|e| io_error("Write temporary settings", e))?;
            file.sync_all()
                .map_err(|e| io_error("Flush temporary settings", e))?;
        }
        drop(pending.file.take());
        fs::rename(&pending.path, path).map_err(|e| io_error("Replace settings", e))?;
        pending.published = true;
        return Ok(());
    }
    Err(Error::msg(
        "Create temporary settings",
        "16 temporary names already exist",
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    // External JSON and failed replacement cannot be covered reliably by a visual UI check.
    #[test]
    fn portable_json_preserves_unknown_keys_and_failed_save_preserves_state() {
        let dir = std::env::temp_dir().join(format!("hwid-settings-test-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("settings.json");
        fs::write(
            &path,
            br#"{"future":{"name":"retained","n":42},"check_updates_on_start":false}"#,
        )
        .unwrap();
        let mut settings = Settings::read(&path).unwrap();
        settings.save_to(&path, true).unwrap();
        let bytes = fs::read(&path).unwrap();
        assert!(!bytes.starts_with(&[0xef, 0xbb, 0xbf]));
        assert!(bytes.windows(2).any(|pair| pair == b"\r\n"));
        assert!(
            bytes
                .iter()
                .enumerate()
                .all(|(i, &b)| b != b'\n' || i > 0 && bytes[i - 1] == b'\r')
        );
        let reloaded = Settings::read(&path).unwrap();
        assert!(reloaded.check_updates_on_start());
        assert_eq!(reloaded.values["future"]["name"], "retained");
        assert_eq!(reloaded.values["future"]["n"], 42);
        // Replacing a directory must fail without losing either persisted or in-memory data.
        assert!(settings.save_to(&dir, false).is_err());
        assert!(settings.check_updates_on_start());
        assert_eq!(fs::read(&path).unwrap(), bytes);
        settings.save_to(&path, false).unwrap();
        assert!(!Settings::read(&path).unwrap().check_updates_on_start());
        for invalid in ["null", "[]", "{", r#"{"check_updates_on_start":"true"}"#] {
            fs::write(&path, invalid).unwrap();
            assert!(Settings::read(&path).is_err(), "{invalid}");
        }
        fs::write(&path, "{}").unwrap();
        assert!(!Settings::read(&path).unwrap().check_updates_on_start());
        assert_eq!(
            fs::read_dir(&dir).unwrap().count(),
            1,
            "temporary writes cleaned up"
        );
        fs::remove_file(path).unwrap();
        fs::remove_dir(dir).unwrap();
    }
}
