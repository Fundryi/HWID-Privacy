//! DISK DRIVES: legacy tree rendering with native storage and WMI enrichment.

use crate::{
    hw::{Ctx, first_ok},
    report::{Out, trim_net},
    win::{
        self, Error, process, storage,
        wmi::{self, Namespace},
    },
};
use std::{collections::HashMap, time::Duration};

struct LogicalDrive {
    physical: String,
    letter: String,
    serial: String,
}

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
        out.text("No disk drives detected.");
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
    if !EXPECTED_UNSUPPORTED.contains(&error.code) {
        disk.failures.push(format!("    {label}: {error}"));
    }
}

fn nonempty(value: &str) -> &str {
    if value.is_empty() { "<empty>" } else { value }
}

fn render_disks(out: &mut Out, disks: &[DiskInfo]) {
    // C# parity: Hardware/DiskDriveInfo.cs:40-120 (50 dashes, tree, field order).
    out.text("Device ID").text(&"-".repeat(50));
    for (i, disk) in disks.iter().enumerate() {
        if i != 0 {
            out.text(&"-".repeat(50));
        }
        out.text(&format!("└── {}", disk.device_id.replace(r"\\.\", "")));
        if disk.volumes.is_empty() {
            out.text("    ├── Drive: ").text("    │   └── Volume-SN: ");
        }
        for (letter, serial) in &disk.volumes {
            out.text(&format!("    ├── Drive: {letter}"));
            out.text(&format!("    │   └── Volume-SN: {serial}"))
                .id_value(serial);
        }
        out.text(&format!("    ├── Model: {}", disk.model));
        out.text(&format!("    ├── Serial: {}", disk.serial))
            .id_value(&disk.serial);
        if let Some((id, identifier)) = &disk.hardware_id {
            out.text(&format!("    ├── Hardware ID: {id}"));
            // An error is display text, not an identifier.
            if *identifier {
                out.id_value(id);
            }
        }
        let prefix = if disk.details.is_empty() {
            "└──"
        } else {
            "├──"
        };
        out.text(&format!("    {prefix} Firmware: {}", disk.firmware));
        for (i, (label, value, identifier)) in disk.details.iter().enumerate() {
            let prefix = if i + 1 == disk.details.len() {
                "└──"
            } else {
                "├──"
            };
            out.text(&format!("    {prefix} {label}: {value}"));
            if *identifier {
                out.id_value(value);
            }
        }
        // AD-03 errors follow the intact tree; failures do not change C#'s
        // successful field values or its last-detail/Firmware tree connectors.
        for failure in &disk.failures {
            out.text(failure);
        }
    }
}

fn logical_drives(
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
                        failures.push(format!("Drive {letter}: {error}"));
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

fn logical_drive_wmi(letter: char) -> win::Result<Vec<LogicalDrive>> {
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

fn unique_ids_wmi() -> win::Result<HashMap<u32, String>> {
    let mut ids = HashMap::new();
    for row in wmi::query(
        Namespace::Storage,
        "SELECT DeviceId, UniqueId FROM MSFT_PhysicalDisk",
    )? {
        if let (Some(index), Some(id)) = (row.str("DeviceId"), row.str("UniqueId")) {
            insert_unique_id(&mut ids, &index, &id);
        }
    }
    // C# parity: Hardware/DiskDriveInfo.cs:209-236. Empty success does not fallback.
    Ok(ids)
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

fn unique_ids_powershell() -> win::Result<HashMap<u32, String>> {
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

fn parse_unique_ids_json(text: &str) -> win::Result<HashMap<u32, String>> {
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

fn convert_unique_id_to_hex(id: &str) -> String {
    // C# parity: Hardware/DiskDriveInfo.cs:294-327. GUIDs use ToByteArray mixed
    // endian; other strings truncate each UTF-16 unit, not Unicode scalar/UTF-8.
    let bytes = guid_bytes(trim_net(id))
        .map(Vec::from)
        .unwrap_or_else(|| id.encode_utf16().map(|unit| unit as u8).collect());
    storage::colon_hex(&bytes)
}

fn guid_bytes(text: &str) -> Option<[u8; 16]> {
    if text.encode_utf16().count() < 32 {
        return None;
    }
    match text.as_bytes().first()? {
        b'(' => guid_d(text.strip_prefix('(')?.strip_suffix(')')?),
        b'{' if text.as_bytes().get(9) == Some(&b'-') => {
            guid_d(text.strip_prefix('{')?.strip_suffix('}')?)
        }
        b'{' => guid_x(text),
        _ if text.as_bytes().get(8) == Some(&b'-') => guid_d(text),
        _ => {
            if text.len() != 32 || !text.bytes().all(|b| b.is_ascii_hexdigit()) {
                return None;
            }
            let dashed = format!(
                "{}-{}-{}-{}-{}",
                &text[..8],
                &text[8..12],
                &text[12..16],
                &text[16..20],
                &text[20..]
            );
            guid_d(&dashed)
        }
    }
}

fn guid_d(text: &str) -> Option<[u8; 16]> {
    if text.len() != 36
        || !text.is_ascii()
        || [8, 13, 18, 23].iter().any(|&i| text.as_bytes()[i] != b'-')
    {
        return None;
    }
    let mut bytes = [0; 16];
    // Guid.TryParse D/B/P also accepts '+' and '0x' prefixes inside the fixed
    // widths, except in the last eight digits (.NET 10 Guid.TryCompatParsing).
    bytes[..4].copy_from_slice(&guid_hex(&text[..8])?.to_le_bytes());
    bytes[4..6].copy_from_slice(&(guid_hex(&text[9..13])? as u16).to_le_bytes());
    bytes[6..8].copy_from_slice(&(guid_hex(&text[14..18])? as u16).to_le_bytes());
    bytes[8..10].copy_from_slice(&(guid_hex(&text[19..23])? as u16).to_be_bytes());
    bytes[10..12].copy_from_slice(&(guid_hex(&text[24..28])? as u16).to_be_bytes());
    if !text[28..].bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    bytes[12..].copy_from_slice(&u32::from_str_radix(&text[28..], 16).ok()?.to_be_bytes());
    Some(bytes)
}

fn guid_hex(text: &str) -> Option<u32> {
    let text = text.strip_prefix('+').unwrap_or(text);
    let text = text
        .strip_prefix("0x")
        .or_else(|| text.strip_prefix("0X"))
        .unwrap_or(text);
    // .NET accepts a prefix without following digits as zero.
    text.bytes().try_fold(0u32, |n, b| {
        n.checked_mul(16)?.checked_add(char::from(b).to_digit(16)?)
    })
}

fn guid_x(text: &str) -> Option<[u8; 16]> {
    // C# parity: Hardware/DiskDriveInfo.cs:302 (Guid.TryParse, including X's
    // whitespace, 32-bit short-field truncation and redundant hexadecimal prefix).
    let compact: String = text.chars().filter(|c| !c.is_whitespace()).collect();
    let inner = compact.strip_prefix('{')?.strip_suffix("}}")?;
    let (head, tail) = inner.split_once(",{")?;
    let head: Vec<_> = head.split(',').collect();
    let tail: Vec<_> = tail.split(',').collect();
    if head.len() != 3 || tail.len() != 8 {
        return None;
    }
    let component = |s: &str| {
        let digits = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X"))?;
        if digits.is_empty() {
            return None;
        }
        guid_hex(digits)
    };
    let mut bytes = [0; 16];
    bytes[..4].copy_from_slice(&component(head[0])?.to_le_bytes());
    bytes[4..6].copy_from_slice(&(component(head[1])? as u16).to_le_bytes());
    bytes[6..8].copy_from_slice(&(component(head[2])? as u16).to_le_bytes());
    for (i, text) in tail.iter().enumerate() {
        bytes[8 + i] = u8::try_from(component(text)?).ok()?;
    }
    Some(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hw::{self, PROVIDERS};
    use serde_json::Value;

    #[test]
    fn unique_id_guid_and_json_fixtures() {
        let fixture: Value =
            serde_json::from_str(include_str!("../../tests/fixtures/wp-01/unique-ids.json"))
                .expect("fixture JSON");
        for case in fixture["cases"].as_array().expect("GUID cases") {
            assert_eq!(
                convert_unique_id_to_hex(case["input"].as_str().expect("input")),
                case["hex"],
                "{}",
                case["name"]
            );
        }
        for case in fixture["json"].as_array().expect("JSON cases") {
            let result = parse_unique_ids_json(case["input"].as_str().expect("JSON input"));
            if case["error"] == true {
                assert!(result.is_err(), "{}", case["name"]);
            } else {
                assert_eq!(
                    serde_json::to_value(result.expect("valid JSON")).expect("map"),
                    case["expected"],
                    "{}",
                    case["name"]
                );
            }
        }
    }

    #[test]
    fn disk_tree_text_and_identifier_records() {
        let disks = [
            DiskInfo {
                device_id: r"\\.\PHYSICALDRIVE7".into(),
                model: "NVMe Example 2TB".into(),
                serial: "S6P8NX0W418725".into(),
                firmware: "7B2QEXM7".into(),
                hardware_id: Some(("SCSI\\DiskNVMe_Example_2TB".into(), true)),
                failures: vec![],
                volumes: vec![
                    ("C".into(), "A1B2C3D4".into()),
                    ("F".into(), "79A34D81".into()),
                ],
                details: vec![
                    (
                        "UniqueId (IOCTL)".into(),
                        "50:00:C5:02:A9:37:16:B2".into(),
                        true,
                    ),
                    ("UniqueId (IOCTL) decoded".into(), "<empty>".into(), false),
                    (
                        "Partition Style: GPT | Disk GUID".into(),
                        "72C3A490-8B16-4DF2-9A0C-6F21B497E853".into(),
                        true,
                    ),
                    (
                        "  Partition GUID".into(),
                        "5E7AC148-3B72-4CB4-892A-73A1F624B8D9".into(),
                        true,
                    ),
                ],
            },
            DiskInfo {
                device_id: r"\\.\PHYSICALDRIVE9".into(),
                model: "Unknown Model".into(),
                serial: "Unknown Serial".into(),
                firmware: String::new(),
                hardware_id: None,
                volumes: vec![],
                details: vec![],
                failures: vec![
                    "    UniqueId (IOCTL): storage query failed: 0x00000005 fabricated denial"
                        .into(),
                ],
            },
        ];
        let mut out = Out::new();
        render_disks(&mut out, &disks);
        let section = out.finish();
        let expected: Value =
            serde_json::from_str(include_str!("../../tests/fixtures/wp-01/disk-tree.json"))
                .expect("tree fixture");
        assert_eq!(section.body, expected["body"]);
        for value in [
            "A1B2C3D4",
            "79A34D81",
            "S6P8NX0W418725",
            "SCSI\\DiskNVMe_Example_2TB",
            "50:00:C5:02:A9:37:16:B2",
            "72C3A490-8B16-4DF2-9A0C-6F21B497E853",
            "5E7AC148-3B72-4CB4-892A-73A1F624B8D9",
        ] {
            assert!(
                section.ids.iter().any(|id| id == value),
                "unrecorded identifier: {value}"
            );
        }
        assert!(!section.ids.iter().any(|id| id == "<empty>"));
    }

    #[test]
    #[ignore = "read-only live capture; prints real identifiers, redirect output to private golden/wp-01"]
    fn wp01_capture_nonadmin() {
        use std::{sync::mpsc, thread};
        let (send, receive) = mpsc::sync_channel(1);
        thread::spawn(move || {
            capture_nonadmin();
            let _ = send.send(()); // The capture may have timed out.
        });
        receive
            .recv_timeout(Duration::from_secs(60))
            .expect("provider and source-comparison capture deadline");
    }

    fn capture_nonadmin() {
        use std::{fs, path::Path};
        let folder = Path::new(r"D:\GIT\HWID-Privacy\app\rust\golden\wp-01");
        fs::create_dir_all(folder).expect("private capture folder");
        let section = hw::collect_provider(&PROVIDERS[0], &Ctx::new());
        fs::write(folder.join("rust-nonadmin.txt"), &section.body).expect("private text");
        fs::write(
            folder.join("rust-nonadmin.diag.txt"),
            format!(
                "elevated={}\r\nelapsed_ms={}\r\nsource={}\r\n{}\r\n",
                win::security::is_admin(),
                section.elapsed_ms,
                section.source,
                section.failures.join("\r\n")
            ),
        )
        .expect("private diagnostics");
        println!("{}", section.body);
        assert!(
            !section
                .body
                .contains("Error retrieving DISK DRIVES information:")
        );

        // Independently compare native volume values to the legacy WMI source.
        let mut checks = String::new();
        for letter in storage::drive_letters().expect("drive letters") {
            match storage::volume(letter) {
                Ok(Some(native)) => match logical_drive_wmi(letter) {
                    Ok(wmi) => {
                        let same = wmi.iter().any(|v| {
                            v.physical == format!(r"\\.\PHYSICALDRIVE{}", native.disk_number)
                                && v.serial == native.serial
                        });
                        checks.push_str(&format!(
                            "{letter}: native disk={} serial={} WMI equal={same}\r\n",
                            native.disk_number, native.serial
                        ));
                        for row in wmi {
                            checks.push_str(&format!(
                                "  WMI {} {} {}\r\n",
                                row.physical, row.letter, row.serial
                            ));
                        }
                    }
                    Err(error) => checks.push_str(&format!(
                        "{letter}: WMI comparison unavailable: {error}\r\n"
                    )),
                },
                Ok(None) => {}
                Err(error) => checks.push_str(&format!(
                    "{letter}: native comparison unavailable: {error}\r\n"
                )),
            }
        }
        let wmi_ids = unique_ids_wmi();
        let ps_ids = unique_ids_powershell();
        checks.push_str(&format!(
            "Storage WMI vs PowerShell UniqueIds equal={}\r\n",
            matches!((&wmi_ids, &ps_ids), (Ok(a), Ok(b)) if a == b)
        ));
        checks.push_str(&format!("WMI: {wmi_ids:?}\r\nPowerShell: {ps_ids:?}\r\n"));
        fs::write(folder.join("source-comparison.txt"), checks).expect("private comparison");
    }
}
