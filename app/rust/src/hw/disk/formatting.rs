//! Legacy disk-tree rendering and .NET-compatible unique-ID formatting.

use super::DiskInfo;
use crate::{
    report::{Out, trim_net},
    win::storage,
};

/// Preserves the legacy placeholder for an empty decoded unique ID.
pub(super) fn nonempty(value: &str) -> &str {
    if value.is_empty() { "<empty>" } else { value }
}

/// Writes the legacy disk tree and marks its identifier values.
pub(super) fn render_disks(out: &mut Out, disks: &[DiskInfo]) {
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
        for (label, value) in &disk.nvme_ids {
            out.text(&format!("    ├── {label}: {value}"))
                .id_value(value);
        }
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

/// Adds nonempty VPD values only when this disk does not already display them.
pub(super) fn add_storage_identifiers(
    out: &mut Out,
    disk: &mut DiskInfo,
    descriptors: &[storage::StorageIdentifier],
) {
    for descriptor in descriptors {
        let value = match descriptor_value(descriptor) {
            Ok(Some(value)) => value,
            Ok(None) => continue,
            Err(error) => {
                out.fallback_failed(&format!("{} Storage Identifier", disk.device_id), &error);
                disk.failures
                    .push(format!("    Storage Identifier: Unavailable ({error})"));
                continue;
            }
        };
        if identity_displayed(disk, &value) {
            continue;
        }
        // SCSI association 0 / Windows Device identifies a logical unit, 1 a
        // target port, 2 the target device. Never relabel a target/port as a LU.
        let association = match descriptor.association {
            0 => "device / logical unit".into(),
            1 => "port".into(),
            2 => "target device".into(),
            n => format!("association {n}"),
        };
        let kind = match descriptor.identifier_type {
            0 => "vendor-specific".into(),
            1 => "vendor ID".into(),
            2 => "EUI-64".into(),
            3 => "NAA".into(),
            4 => "relative port".into(),
            5 => "port group".into(),
            6 => "logical unit group".into(),
            7 => "MD5 logical unit".into(),
            8 => "SCSI name".into(),
            n => format!("type {n}"),
        };
        let code = match descriptor.code_set {
            1 => "binary",
            2 => "ASCII",
            3 => "UTF-8",
            _ => unreachable!(),
        };
        disk.details.push((
            format!("Storage ID ({association}, {kind}, {code})"),
            value,
            true,
        ));
    }
}

pub(super) fn identity_displayed(disk: &DiskInfo, value: &str) -> bool {
    let same = |old: &str| {
        value == old
            || matches!((hex_identity(value), hex_identity(old)),
            (Some(a), Some(b)) if a == b)
    };
    same(&disk.serial)
        || disk.nvme_ids.iter().any(|(_, old)| same(old))
        || disk.details.iter().any(|(_, old, id)| *id && same(old))
        || disk.volumes.iter().any(|(_, old)| same(old))
        || disk
            .hardware_id
            .as_ref()
            .is_some_and(|(old, id)| *id && same(old))
}

fn descriptor_value(descriptor: &storage::StorageIdentifier) -> crate::win::Result<Option<String>> {
    let value = &descriptor.value;
    if value.is_empty() || value.iter().all(|&b| b == 0) {
        return Ok(None);
    }
    if descriptor.code_set == 1 {
        return Ok(Some(storage::colon_hex(value)));
    }
    if !matches!(descriptor.code_set, 2 | 3) {
        return Err(crate::win::Error::msg(
            "storage descriptor",
            "invalid identifier code set",
        ));
    }
    let end = value.iter().rposition(|&b| b != 0).map_or(0, |i| i + 1);
    let value = &value[..end];
    let text = std::str::from_utf8(value).map_err(|_| {
        crate::win::Error::msg("storage descriptor", "invalid identifier text encoding")
    })?;
    if text.chars().any(char::is_control) || descriptor.code_set == 2 && !text.is_ascii() {
        return Err(crate::win::Error::msg(
            "storage descriptor",
            "invalid identifier text encoding",
        ));
    }
    Ok((!trim_net(text).is_empty()).then(|| text.to_owned()))
}

// Compare only explicit hex encodings; arbitrary serial punctuation/case stays.
fn hex_identity(value: &str) -> Option<String> {
    let value = value
        .strip_prefix("eui.")
        .or_else(|| value.strip_prefix("naa."))
        .or_else(|| value.strip_prefix("uuid."))
        .unwrap_or(value);
    let mut hex = String::new();
    for c in value.chars() {
        if c.is_ascii_hexdigit() {
            hex.push(c.to_ascii_uppercase());
        } else if !matches!(c, ':' | '-' | '_' | '.' | '{' | '}') {
            return None;
        }
    }
    (hex.len() >= 4 && hex.len().is_multiple_of(2)).then_some(hex)
}

/// Formats a unique ID using the legacy GUID or UTF-16 byte conversion.
pub(super) fn convert_unique_id_to_hex(id: &str) -> String {
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
mod descriptor_tests {
    use super::*;

    #[test]
    fn descriptor_text_safety_duplicate_suppression_and_masked_rendering() {
        let uuid = "D197A234-51C8-4E62-9B17-A06432DF8790";
        let mut disk = DiskInfo {
            device_id: "PHYSICALDRIVE0".into(),
            model: "Samsung SSD 980 PRO 1TB".into(),
            serial: "S5GXNF0R913742K".into(),
            nvme_ids: vec![
                ("NVMe Namespace EUI-64", "002538C86B1479A2".into()),
                ("NVMe Namespace UUID", uuid.into()),
            ],
            firmware: "2B2QGXA7".into(),
            hardware_id: None,
            volumes: Vec::new(),
            details: Vec::new(),
            failures: Vec::new(),
        };
        let descriptor =
            |code_set, identifier_type, association, value: &[u8]| storage::StorageIdentifier {
                code_set,
                identifier_type,
                association,
                value: value.to_vec(),
            };
        let mut out = Out::new();
        add_storage_identifiers(
            &mut out,
            &mut disk,
            &[
                descriptor(3, 8, 0, b"eui.002538c86b1479a2\0"),
                descriptor(2, 1, 0, b"S5GXNF0R913742K"),
                descriptor(2, 1, 1, b"Samsung_Port_4A729C1E\0"),
                descriptor(3, 1, 2, b"Samsung_Port_4A729C1E"),
                descriptor(1, 3, 2, &[0x50, 0x02, 0x53, 0x8E, 0xA7, 0x49, 0x3D, 0x21]),
                descriptor(2, 1, 0, b"\0\0"),
                descriptor(3, 8, 0, &[0xFF]),
                descriptor(2, 1, 0, b"SECRET_BAD\nVALUE"),
            ],
        );
        assert_eq!(disk.details.len(), 2);
        assert_eq!(disk.details[0].0, "Storage ID (port, vendor ID, ASCII)");
        assert_eq!(disk.details[1].0, "Storage ID (target device, NAA, binary)");
        assert_eq!(disk.failures.len(), 2);
        render_disks(&mut out, &[disk]);
        let section = out.finish();
        assert!(!section.body.contains("SECRET_BAD"));
        assert!(!section.body.contains("eui."));
        let masked = crate::report::masked(&section);
        for id in [uuid, "Samsung_Port_4A729C1E", "50:02:53:8E:A7:49:3D:21"] {
            assert!(section.ids.iter().any(|marked| marked == id));
            assert!(section.body.contains(id));
            assert!(!masked.body.contains(id));
        }
        assert!(
            masked
                .body
                .contains("Storage ID (target device, NAA, binary)")
        );
    }
}
