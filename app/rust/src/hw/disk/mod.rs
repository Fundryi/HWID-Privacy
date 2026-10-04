//! DISK DRIVES: section collection and source enrichment.

mod formatting;
mod sources;
#[cfg(test)]
mod tests;

use crate::{
    hw::{Ctx, first_ok},
    report::{Out, trim_net},
    win::{
        self, Error, storage,
        wmi::{self, Namespace},
    },
};
use formatting::{convert_unique_id_to_hex, nonempty, render_disks};
use sources::{logical_drives, unique_ids_powershell, unique_ids_wmi};
use std::collections::HashMap;

struct DiskInfo {
    device_id: String,
    model: String,
    serial: String,
    firmware: String,
    hardware_id: Option<(String, bool)>,
    volumes: Vec<(String, String)>,
    details: Vec<(String, String, bool)>,
    failures: Vec<String>,
}

/// Collects this hardware section through the shared output builder.
pub fn collect(ctx: &Ctx, out: &mut Out) -> Result<(), win::Error> {
    let rows = wmi::query(Namespace::Cimv2, "SELECT * FROM Win32_DiskDrive")?;
    out.source("WMI (Win32_DiskDrive)");
    if rows.is_empty() {
        // C# parity: Hardware/DiskDriveInfo.cs:42. The empty body has no final CRLF.
        out.text("No disk drives detected.").trim_end();
        return Ok(());
    }
    let mut sources = vec!["WMI (Win32_DiskDrive)"];
    let mut failures = Vec::new();
    let volumes = logical_drives(out, &mut sources, &mut failures);
    let unique_ids = match first_ok(
        out,
        "MSFT_PhysicalDisk",
        &[
            ("WMI (Storage)", &|| {
                unique_ids_wmi().map(|m| (m, "WMI (Storage)"))
            }),
            ("PowerShell (Get-PhysicalDisk)", &|| {
                unique_ids_powershell().map(|m| (m, "PowerShell (Get-PhysicalDisk)"))
            }),
        ],
    ) {
        Ok((ids, source)) => {
            sources.push(source);
            ids
        }
        Err(error) => {
            failures.push(format!("UniqueId (WMI): {error}"));
            HashMap::new()
        }
    };
    let hardware_ids = ctx.hardware_ids();
    match &hardware_ids {
        Ok(_) => sources.push("SetupAPI (Ctx)"),
        Err(error) => {
            out.fallback_failed("SetupAPI hardware IDs", error);
        }
    }
    let mut disks = Vec::new();
    for row in rows {
        // C# parity: Hardware/DiskDriveInfo.cs:134-138. Null differs from empty;
        // OEM placeholders and whitespace-trimmed empty strings are preserved.
        let device_id = row
            .str("DeviceID")
            .unwrap_or_else(|| "Unknown Device".into());
        let value = |name, default: &str| {
            row.str(name)
                .map(|v| trim_net(&v).to_owned())
                .unwrap_or_else(|| default.to_owned())
        };
        let mut disk = DiskInfo {
            volumes: volumes
                .iter()
                .filter(|v| v.physical == device_id)
                .map(|v| (v.letter.clone(), v.serial.clone()))
                .collect(),
            hardware_id: match &hardware_ids {
                Ok(ids) => row
                    .str("PNPDeviceID")
                    .and_then(|p| ids.get(&p.to_uppercase()).cloned())
                    .filter(|id| !id.is_empty())
                    .map(|id| (id, true)),
                Err(error) => Some((error.to_string(), false)),
            },
            device_id,
            model: value("Model", "Unknown Model"),
            serial: value("SerialNumber", "Unknown Serial"),
            firmware: value("FirmwareRevision", ""),
            details: Vec::new(),
            failures: Vec::new(),
        };
        // C# parity: Hardware/DiskDriveInfo.cs:140-144 (signed Int32 index).
        let index = row
            .str("Index")
            .and_then(|v| trim_net(&v).parse::<i32>().ok())
            .filter(|&n| n >= 0);
        if let Some(index) = index {
            match storage::physical_identifier(index as u32) {
                Ok(Some(id)) => {
                    sources.push("native (storage)");
                    if !id.hex.is_empty() {
                        disk.details.push(("UniqueId (IOCTL)".into(), id.hex, true));
                        disk.details.push((
                            "UniqueId (IOCTL) decoded".into(),
                            nonempty(&id.decoded).into(),
                            !id.decoded.is_empty(),
                        ));
                    }
                }
                Ok(None) => {
                    sources.push("native (storage)");
                }
                Err(error) => disk_error(out, &mut disk, "UniqueId (IOCTL)", &error),
            }
            if let Some(id) = unique_ids.get(&(index as u32)) {
                disk.details
                    .push(("UniqueId (WMI)".into(), convert_unique_id_to_hex(id), true));
                disk.details.push((
                    "UniqueId (WMI) decoded".into(),
                    nonempty(id).into(),
                    !id.is_empty(),
                ));
            }
            match storage::physical_layout(index as u32) {
                Ok(Some(layout)) => {
                    sources.push("native (storage)");
                    let label = if layout.is_gpt {
                        "Partition Style: GPT | Disk GUID"
                    } else {
                        "Partition Style: MBR | Disk Signature"
                    };
                    disk.details.push((label.into(), layout.disk_id, true));
                    for guid in layout.partition_guids {
                        disk.details.push(("  Partition GUID".into(), guid, true));
                    }
                }
                Ok(None) => {
                    sources.push("native (storage)");
                }
                Err(error) => disk_error(out, &mut disk, "Partition Style", &error),
            }
        } else {
            disk_error(
                out,
                &mut disk,
                "Disk Index",
                &Error::msg(
                    "Win32_DiskDrive Index",
                    "missing or invalid disk index; storage identities unavailable",
                ),
            );
        }
        disks.push(disk);
    }
    render_disks(out, &disks);
    for failure in failures {
        out.text(&failure);
    }
    // Out stores a single source string, so retain every successful source here.
    let mut distinct = Vec::new();
    for source in sources {
        if !distinct.contains(&source) {
            distinct.push(source);
        }
    }
    out.source(&distinct.join("; "));
    Ok(())
}

/// Win32 codes a healthy disk returns when it lacks a feature or has no media:
/// invalid function, not ready, not supported, no media in drive.
const EXPECTED_UNSUPPORTED: [u32; 4] = [1, 21, 50, 1112];

fn disk_error(out: &mut Out, disk: &mut DiskInfo, label: &str, error: &Error) {
    out.fallback_failed(&format!("{} {label}", disk.device_id), error);
    // C# parity: Hardware/DiskDriveInfo.cs shows nothing when a disk does not
    // support a query; keep that case in `.diag.txt` only.
    // USB bridges (USBSTOR) reject the unique-ID property with ERROR_INVALID_PARAMETER.
    let unsupported_id = label == "UniqueId (IOCTL)" && error.code == 87;
    if !EXPECTED_UNSUPPORTED.contains(&error.code) && !unsupported_id {
        disk.failures.push(format!("    {label}: {error}"));
    }
}
