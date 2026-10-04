//! Volume associations and physical-disk unique-ID sources.

use crate::{
    report::{Out, trim_net},
    win::{
        self, Error, process, storage,
        wmi::{self, Namespace},
    },
};
use std::{collections::HashMap, time::Duration};

pub(super) struct LogicalDrive {
    pub(super) physical: String,
    pub(super) letter: String,
    pub(super) serial: String,
}

/// Collects sorted volume mappings, retaining source and failure diagnostics.
pub(super) fn logical_drives(
    out: &mut Out,
    sources: &mut Vec<&'static str>,
    failures: &mut Vec<String>,
) -> Vec<LogicalDrive> {
    let mut result = Vec::new();
    let letters = match storage::drive_letters() {
        Ok(letters) => letters,
        Err(error) => {
            out.fallback_failed("native drive letters", &error);
            failures.push(format!("Drive letters: {error}"));
            return result;
        }
    };
    for letter in letters {
        match storage::volume(letter) {
            Ok(Some(volume)) => {
                sources.push("native (volumes)");
                result.push(LogicalDrive {
                    physical: format!(r"\\.\PHYSICALDRIVE{}", volume.disk_number),
                    letter: letter.to_string(),
                    serial: volume.serial,
                });
            }
            Ok(None) => {}
            Err(error) => {
                out.fallback_failed(&format!("native volume {letter}:"), &error);
                match logical_drive_wmi(letter) {
                    Ok(mut volumes) => {
                        if !volumes.is_empty() {
                            sources.push("WMI (volume associations)");
                        }
                        result.append(&mut volumes);
                    }
                    Err(error) => {
                        out.fallback_failed(&format!("WMI volume {letter}:"), &error);
                        // AD-46: C# loses the section when this association query fails.
                        failures.push(format!("Drive {letter}: Unavailable ({error})"));
                    }
                }
            }
        }
    }
    // AD-06: every letter, ascending; one volume may associate with several
    // partitions on the same disk, but its Drive/Volume-SN pair appears once.
    result.sort_by(|a, b| a.letter.cmp(&b.letter).then(a.physical.cmp(&b.physical)));
    result.dedup_by(|a, b| a.physical == b.physical && a.letter == b.letter);
    result
}

/// Resolves one drive letter through the legacy WMI association path.
pub(super) fn logical_drive_wmi(letter: char) -> win::Result<Vec<LogicalDrive>> {
    // C# parity: Hardware/DiskDriveInfo.cs:330-357 (per-volume association path).
    let logical = wmi::query(
        Namespace::Cimv2,
        &format!("SELECT * FROM Win32_LogicalDisk WHERE DeviceID='{letter}:'"),
    )?;
    let serial = logical
        .first()
        .and_then(|r| r.str("VolumeSerialNumber"))
        .unwrap_or_default();
    let partitions = wmi::query(
        Namespace::Cimv2,
        &format!(
            "ASSOCIATORS OF {{Win32_LogicalDisk.DeviceID='{letter}:'}} WHERE AssocClass = Win32_LogicalDiskToPartition"
        ),
    )?;
    let mut volumes = Vec::new();
    for partition in partitions {
        let id = partition
            .str("DeviceID")
            .ok_or_else(|| Error::msg("Win32_DiskPartition", "missing DeviceID"))?;
        let id = id.replace('\\', "\\\\").replace('\'', "\\'");
        let disks = wmi::query(
            Namespace::Cimv2,
            &format!(
                "ASSOCIATORS OF {{Win32_DiskPartition.DeviceID='{id}'}} WHERE AssocClass = Win32_DiskDriveToDiskPartition"
            ),
        )?;
        for disk in disks {
            volumes.push(LogicalDrive {
                physical: disk.str("DeviceID").unwrap_or_else(|| "Unknown".into()),
                letter: letter.to_string(),
                serial: serial.clone(),
            });
        }
    }
    // C# parity: Hardware/DiskDriveInfo.cs:344-354. No association (empty card
    // reader, subst or RAM-disk letter) is the normal no-mapping path, not an error.
    Ok(volumes)
}

/// Independent fields joined using the existing physical-disk index mapping.
#[derive(Default)]
pub(super) struct PhysicalDiskIds {
    pub(super) unique_ids: HashMap<u32, String>,
    pub(super) adapter_serials: HashMap<u32, String>,
}

/// Reads physical-disk unique IDs and adapter serials from the Storage namespace.
pub(super) fn physical_disk_ids_wmi() -> win::Result<PhysicalDiskIds> {
    let mut ids = PhysicalDiskIds::default();
    for row in wmi::query(
        Namespace::Storage,
        "SELECT DeviceId, UniqueId, AdapterSerialNumber FROM MSFT_PhysicalDisk",
    )? {
        if let Some(index) = row.str("DeviceId") {
            if let Some(id) = row.str("UniqueId") {
                insert_unique_id(&mut ids.unique_ids, &index, &id);
            }
            if let Some(serial) = row.str("AdapterSerialNumber") {
                insert_unique_id(&mut ids.adapter_serials, &index, trim_net(&serial));
            }
        }
    }
    // C# parity: Hardware/DiskDriveInfo.cs:209-236. Empty success does not fallback.
    Ok(ids)
}

/// Keeps the existing source-comparison check's UniqueId-only contract.
#[cfg(test)]
pub(super) fn unique_ids_wmi() -> win::Result<HashMap<u32, String>> {
    physical_disk_ids_wmi().map(|ids| ids.unique_ids)
}

fn insert_unique_id(ids: &mut HashMap<u32, String>, index: &str, id: &str) {
    // C# parity: Hardware/DiskDriveInfo.cs:222-226 (do not trim the UniqueId).
    if let Ok(index) = trim_net(index).parse::<i32>()
        && index >= 0
        && !id.is_empty()
    {
        ids.insert(index as u32, id.to_owned());
    }
}

/// Reads physical-disk unique IDs through the bounded PowerShell fallback.
pub(super) fn unique_ids_powershell() -> win::Result<HashMap<u32, String>> {
    let system_exe = process::system32("powershell.exe");
    let directory = system_exe
        .parent()
        .ok_or_else(|| Error::msg("Get-PhysicalDisk", "System32 path unavailable"))?;
    let output = process::run(
        &directory.join(r"WindowsPowerShell\v1.0\powershell.exe"),
        &["-NoProfile", "-NonInteractive", "-Command", "[Console]::OutputEncoding = [System.Text.UTF8Encoding]::new($false); $ErrorActionPreference='Stop'; Get-PhysicalDisk | Select-Object DeviceId, UniqueId | ConvertTo-Json"],
        Duration::from_secs(15), &process::Cancel::new(),
    ).map_err(|e| Error::msg("Get-PhysicalDisk", e))?;
    if output.code != 0 {
        return Err(Error::msg(
            "Get-PhysicalDisk",
            format!("exit {}: {}", output.code, output.stderr.trim()),
        ));
    }
    parse_unique_ids_json(&output.stdout)
}

/// Parses either the single-disk object or multi-disk array returned by PowerShell.
pub(super) fn parse_unique_ids_json(text: &str) -> win::Result<HashMap<u32, String>> {
    let mut ids = HashMap::new();
    if text.trim().is_empty() {
        return Ok(ids);
    }
    let value: serde_json::Value = serde_json::from_str(text.trim_start_matches('\u{feff}'))
        .map_err(|e| Error::msg("Get-PhysicalDisk JSON", e.to_string()))?;
    let rows = match &value {
        serde_json::Value::Array(rows) => rows.as_slice(),
        serde_json::Value::Object(_) => std::slice::from_ref(&value),
        serde_json::Value::Null => return Ok(ids),
        _ => {
            return Err(Error::msg(
                "Get-PhysicalDisk JSON",
                "expected an object or array",
            ));
        }
    };
    for row in rows {
        if !row.is_object() {
            return Err(Error::msg(
                "Get-PhysicalDisk JSON",
                "expected a disk object",
            ));
        }
        let text = |key: &str| {
            row.get(key).filter(|v| !v.is_null()).map(|v| {
                v.as_str()
                    .map(str::to_owned)
                    .unwrap_or_else(|| v.to_string())
            })
        };
        if let (Some(index), Some(id)) = (text("DeviceId"), text("UniqueId")) {
            insert_unique_id(&mut ids, &index, &id);
        }
    }
    Ok(ids)
}
