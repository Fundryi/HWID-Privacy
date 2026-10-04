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
