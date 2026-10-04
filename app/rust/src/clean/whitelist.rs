//! Owned by WP-11: fail-closed whitelist files.

use super::devices::{GhostDevice, Presence};
use crate::win::{self, Error};
use serde::{Deserialize, Serialize};
use std::{
    borrow::Cow,
    fs::{self, File, OpenOptions},
    io::{self, Write},
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

/// The fail-closed message shown instead of offering removal after a whitelist read error.
pub const READ_FAILURE: &str = "Whitelist file could not be read. No devices will be removed.";

// C# parity: app/src/Services/DeviceWhitelistService.cs:85-101. Field order and
// PascalCase names are the on-disk contract; no removal token is serialized.
#[derive(Deserialize, Serialize)]
#[serde(rename_all = "PascalCase")]
struct Entry {
    name: String,
    description: String,
    hardware_id: String,
    class: String,
}

fn path() -> PathBuf {
    // C# parity: app/src/Services/DeviceWhitelistService.cs:17.
    std::env::temp_dir().join("hwid_device_whitelist.json")
}

/// Loads the whitelist, treating missing files as empty and malformed files as errors.
pub fn load_whitelist() -> Result<Vec<GhostDevice>, String> {
    load(&path()).map_err(|error| error.to_string())
}
/// Atomically saves a whitelist through the destructive-operation guard.
pub fn save_whitelist(devices: &[GhostDevice]) -> Result<(), String> {
    let json = encode(devices).map_err(|error| error.to_string())?;
    super::destructive("Save device whitelist", || atomic_write(&path(), &json))
        .unwrap_or_else(|| {
            Err(Error::msg(
                "Save whitelist",
                "destructive operation was not executed",
            ))
        })
        .map_err(|error| error.to_string())
}
/// Resets the whitelist through the destructive-operation guard.
pub fn reset_whitelist() -> Result<(), String> {
    super::destructive("Reset device whitelist", || match fs::remove_file(path()) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(io_error("Reset whitelist", error)),
    })
    .unwrap_or_else(|| {
        Err(Error::msg(
            "Reset whitelist",
            "destructive operation was not executed",
        ))
    })
    .map_err(|error| error.to_string())
}

/// Matches the legacy joined hardware ID exactly, including case and empty strings.
pub fn is_whitelisted(device: &GhostDevice, whitelist: &[GhostDevice]) -> bool {
    // C# parity: app/src/Services/DeviceWhitelistService.cs:60.
    whitelist
        .iter()
        .any(|entry| entry.hardware_id == device.hardware_id)
}

fn load(path: &Path) -> win::Result<Vec<GhostDevice>> {
    match fs::read(path) {
        Ok(bytes) => decode(&bytes),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(Vec::new()),
        Err(error) => Err(io_error("Read whitelist", error)),
    }
}

fn decode(bytes: &[u8]) -> win::Result<Vec<GhostDevice>> {
    let bytes = without_bom(bytes)?;
    // JsonDocument.GetProperty uses the last duplicate key; Value preserves that
    // behavior before the required, case-sensitive string fields are validated.
    // C# parity: app/src/Services/DeviceWhitelistService.cs:83-90.
    let entries: Vec<serde_json::Value> = serde_json::from_slice(&bytes)
        .map_err(|error| Error::msg("Read whitelist JSON", error.to_string()))?;
    let entries: Vec<Entry> = entries
        .into_iter()
        .map(serde_json::from_value)
        .collect::<Result<_, _>>()
        .map_err(|error| Error::msg("Read whitelist JSON", error.to_string()))?;
    Ok(entries
        .into_iter()
        .map(|entry| GhostDevice {
            name: entry.name,
            description: entry.description,
            hardware_id: entry.hardware_id,
            class: entry.class,
            instance_id: String::new(),
            presence: Presence::Unclear,
        })
        .collect())
}

fn without_bom(bytes: &[u8]) -> win::Result<Cow<'_, [u8]>> {
    // File.ReadAllText detects UTF-8, UTF-16 and UTF-32 BOMs; UTF-32 LE must be
    // checked before UTF-16 LE because the first two bytes are identical.
    // C# parity: app/src/Services/DeviceWhitelistService.cs:31.
    let utf32 = if bytes.starts_with(&[0xFF, 0xFE, 0, 0]) {
        Some(true)
    } else if bytes.starts_with(&[0, 0, 0xFE, 0xFF]) {
        Some(false)
    } else {
        None
    };
    let text = if let Some(little) = utf32 {
        let (words, tail) = bytes[4..].as_chunks::<4>();
        if !tail.is_empty() {
            return Err(Error::msg(
                "Read whitelist encoding",
                "incomplete UTF-32 code unit",
            ));
        }
        words
            .iter()
            .map(|word| {
                let value = if little {
                    u32::from_le_bytes(*word)
                } else {
                    u32::from_be_bytes(*word)
                };
                char::from_u32(value)
                    .ok_or_else(|| Error::msg("Read whitelist encoding", "invalid UTF-32 scalar"))
            })
            .collect::<win::Result<String>>()?
    } else if bytes.starts_with(&[0xFF, 0xFE]) || bytes.starts_with(&[0xFE, 0xFF]) {
        let (words, tail) = bytes[2..].as_chunks::<2>();
        if !tail.is_empty() {
            return Err(Error::msg(
                "Read whitelist encoding",
                "incomplete UTF-16 code unit",
            ));
        }
        let units: Vec<_> = words
            .iter()
            .map(|word| {
                if bytes[0] == 0xFF {
                    u16::from_le_bytes(*word)
                } else {
                    u16::from_be_bytes(*word)
                }
            })
            .collect();
        String::from_utf16(&units)
            .map_err(|error| Error::msg("Read whitelist encoding", error.to_string()))?
    } else {
        return Ok(Cow::Borrowed(
            bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]).unwrap_or(bytes),
        ));
    };
    Ok(Cow::Owned(text.into_bytes()))
}

fn encode(devices: &[GhostDevice]) -> win::Result<Vec<u8>> {
    let entries: Vec<_> = devices
        .iter()
        .map(|device| Entry {
            name: device.name.clone(),
            description: device.description.clone(),
            hardware_id: device.hardware_id.clone(),
            class: device.class.clone(),
        })
        .collect();
    // C# parity: app/src/Services/DeviceWhitelistService.cs:48-54. WriteIndented uses
    // Environment.NewLine (CRLF). serde_json escapes LF inside strings, so every raw LF
    // is structural whitespace.
    serde_json::to_string_pretty(&entries)
        .map(|json| json.replace('\n', "\r\n").into_bytes())
        .map_err(|error| Error::msg("Write whitelist JSON", error.to_string()))
}

fn io_error(op: &'static str, error: io::Error) -> Error {
    Error {
        op,
        code: error.raw_os_error().unwrap_or(0) as u32,
        detail: error.to_string(),
    }
}

// The temporary file is exclusively created beside its destination. Drop closes its
// handle before deleting on failure; rename publishes a complete file in one operation.
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
            win::record(io_error("Remove temporary whitelist", error));
        }
    }
}

fn atomic_write(path: &Path, bytes: &[u8]) -> win::Result<()> {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let parent = path
        .parent()
        .ok_or_else(|| Error::msg("Write whitelist", "missing parent folder"))?;
    for _ in 0..16 {
        let temp = parent.join(format!(
            ".hwid_device_whitelist.{}.{}.tmp",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let file = match OpenOptions::new().write(true).create_new(true).open(&temp) {
            Ok(file) => file,
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(io_error("Create temporary whitelist", error)),
        };
        let mut pending = PendingFile {
            path: temp,
            file: Some(file),
            published: false,
        };
        if let Some(file) = &mut pending.file {
            file.write_all(bytes)
                .map_err(|e| io_error("Write temporary whitelist", e))?;
            file.sync_all()
                .map_err(|e| io_error("Flush temporary whitelist", e))?;
        }
        drop(pending.file.take());
        fs::rename(&pending.path, path).map_err(|e| io_error("Replace whitelist", e))?;
        pending.published = true;
        return Ok(());
    }
    Err(Error::msg(
        "Create temporary whitelist",
        "16 temporary names already exist",
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    const LEGACY: &[u8] = include_bytes!("../../tests/fixtures/wp-11/csharp-whitelist.json");
    const RUST: &[u8] = include_bytes!("../../tests/fixtures/wp-11/rust-whitelist.json");

    #[test]
    fn csharp_escapes_bom_and_rust_schema_interoperate() {
        let mut bom = vec![0xEF, 0xBB, 0xBF];
        bom.extend_from_slice(LEGACY);
        let entries = decode(&bom).expect("C# whitelist with BOM");
        assert_eq!(entries[0].description, "USB Gerät & <Reserve> 'A' + 🔒");
        assert_eq!(
            entries[0].hardware_id,
            r"USB\VID_046D&PID_C52B&REV_2401USB\VID_046D&PID_C52B"
        );
        assert!(
            entries
                .iter()
                .all(|d| d.instance_id.is_empty() && d.presence == Presence::Unclear)
        );
        // The fixture's line endings depend on git autocrlf; the written file is always CRLF.
        assert_eq!(
            String::from_utf8(encode(&entries).expect("serialize fixture")).expect("UTF-8 JSON"),
            String::from_utf8_lossy(RUST)
                .trim_end()
                .replace("\r\n", "\n")
                .replace('\n', "\r\n")
        );
        assert!(is_whitelisted(
            &entries[0],
            &decode(RUST).expect("Rust fixture")
        ));
        let mut lowercase = entries[0].clone();
        lowercase.hardware_id.make_ascii_lowercase();
        assert!(!is_whitelisted(&lowercase, &entries));
        assert!(is_whitelisted(&entries[1], &entries));
        for little in [true, false] {
            let text = std::str::from_utf8(LEGACY).expect("UTF-8 fixture");
            let mut utf16 = if little {
                vec![0xFF, 0xFE]
            } else {
                vec![0xFE, 0xFF]
            };
            utf16.extend(text.encode_utf16().flat_map(|unit| {
                if little {
                    unit.to_le_bytes()
                } else {
                    unit.to_be_bytes()
                }
            }));
            assert_eq!(
                encode(&decode(&utf16).expect("UTF-16 fixture")).expect("serialize UTF-16 fixture"),
                encode(&entries).expect("serialize fixture")
            );
            let mut utf32 = if little {
                vec![0xFF, 0xFE, 0, 0]
            } else {
                vec![0, 0, 0xFE, 0xFF]
            };
            utf32.extend(text.chars().flat_map(|ch| {
                if little {
                    (ch as u32).to_le_bytes()
                } else {
                    (ch as u32).to_be_bytes()
                }
            }));
            assert_eq!(
                encode(&decode(&utf32).expect("UTF-32 fixture")).expect("serialize UTF-32 fixture"),
                encode(&entries).expect("serialize fixture")
            );
        }
    }

    #[test]
    fn malformed_whitelist_never_becomes_empty_permission() {
        for bytes in [
            b"".as_slice(),
            b"null",
            b"{}",
            b"[null]",
            b"[{}]",
            b"[",
            b"[] trailing",
            br#"[{"Name":"True","Description":"USB","HardwareId":null,"Class":"USB"}]"#,
            br#"[{"Name":"True","Description":"USB","HardwareId":42,"Class":"USB"}]"#,
            br#"[{"name":"True","Description":"USB","HardwareId":"x","Class":"USB"}]"#,
        ] {
            assert!(decode(bytes).is_err(), "invalid schema accepted");
        }
        assert!(decode(b"[]").expect("empty whitelist").is_empty());
        let folder = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/wp-11");
        assert!(
            load(&folder).is_err(),
            "an unreadable file must not grant removal"
        );
        assert!(
            load(&folder.join("missing-whitelist.json"))
                .expect("missing whitelist")
                .is_empty()
        );
    }
}
