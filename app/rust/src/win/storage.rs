//! Read-only disk queries and checked SDK-layout parsers for WP-01.

use super::{Error, Result, ioctl, wide};
use std::{
    mem::{offset_of, size_of},
    panic::{AssertUnwindSafe, catch_unwind},
    sync::mpsc,
    thread,
    time::Duration,
};
use windows::Win32::{
    Foundation::GENERIC_READ,
    Storage::FileSystem::{
        GetDriveTypeW, GetLogicalDrives, GetVolumeInformationW,
        IOCTL_VOLUME_GET_VOLUME_DISK_EXTENTS,
    },
    System::{
        Diagnostics::Debug::{SEM_FAILCRITICALERRORS, SetThreadErrorMode},
        Ioctl::*,
    },
};
use windows::core::PCWSTR;

/// The selected storage identifier, preserving raw bytes and C# ASCII decoding.
#[derive(Debug, PartialEq, Eq)]
pub struct Identifier {
    /// Uppercase colon-separated bytes, including binary 16-byte identifiers.
    pub hex: String,
    /// ASCII text when the code set or printable-byte check permits it.
    pub decoded: String,
}

/// Disk identity and GPT partition identities in legacy display notation.
#[derive(Debug, PartialEq, Eq)]
pub struct DiskLayout {
    /// True for GPT, false for MBR.
    pub is_gpt: bool,
    /// Uppercase D-format GUID or the MBR signature with its 0x prefix.
    pub disk_id: String,
    /// Nonzero GPT partition GUIDs in on-disk order.
    pub partition_guids: Vec<String>,
}

/// One successfully mapped fixed or removable volume.
pub struct Volume {
    /// Physical disk number returned by both native mapping queries.
    pub disk_number: u32,
    /// Eight uppercase hexadecimal volume-serial digits.
    pub serial: String,
}

// The frozen synchronous IOCTL helper cannot cancel a driver call. Abandon only
// the wait; the worker retains ownership of its buffers/handles until it returns.
fn bounded<T: Send + 'static>(
    op: &'static str,
    work: impl FnOnce() -> Result<T> + Send + 'static,
) -> Result<T> {
    let (send, receive) = mpsc::sync_channel(1);
    thread::Builder::new()
        .name(format!("storage {op}"))
        .spawn(move || {
            let result = catch_unwind(AssertUnwindSafe(|| {
                // SAFETY: This is a dedicated, short-lived OS-query thread. Its error
                // mode disappears with it; no caller or pooled thread is modified.
                unsafe { SetThreadErrorMode(SEM_FAILCRITICALERRORS, None) }
                    .map_err(|e| Error::from_win("SetThreadErrorMode", e))?;
                work()
            }))
            .unwrap_or_else(|_| Err(Error::msg(op, "storage worker panicked")));
            // The caller may have timed out; dropping a late result also drops handles.
            let _ = send.send(result);
        })
        .map_err(|e| Error::msg(op, format!("start worker: {e}")))?;
    receive.recv_timeout(Duration::from_secs(5)).map_err(|e| {
        Error::msg(
            op,
            match e {
                mpsc::RecvTimeoutError::Timeout => "timed out after 5000 ms",
                mpsc::RecvTimeoutError::Disconnected => "storage worker disconnected",
            },
        )
    })?
}

/// Enumerates drive letters in ascending order without probing network volumes.
pub fn drive_letters() -> Result<Vec<char>> {
    // SAFETY: GetLogicalDrives takes no pointers and returns a drive-letter mask.
    let mask = unsafe { GetLogicalDrives() };
    if mask == 0 {
        return Err(Error::last("GetLogicalDrives"));
    }
    Ok((0..26)
        .filter(|i| mask & (1 << i) != 0)
        .map(|i| char::from(b'A' + i))
        .collect())
}

/// Maps a fixed/removable letter; ambiguous or unsupported mappings require WMI.
pub fn volume(letter: char) -> Result<Option<Volume>> {
    if !letter.is_ascii_uppercase() {
        return Err(Error::msg("volume", "invalid drive letter"));
    }
    bounded("volume", move || {
        let root = wide::to_wide(&format!("{letter}:\\"));
        // SAFETY: root is a live, NUL-terminated UTF-16 path.
        let kind = unsafe { GetDriveTypeW(PCWSTR(root.as_ptr())) };
        // DRIVE_REMOVABLE=2, DRIVE_FIXED=3 (winbase.h). The constants' generated
        // WindowsProgramming module is not enabled in the frozen Cargo features.
        if !matches!(kind, 2 | 3) {
            return Ok(None);
        }
        let handle = ioctl::open_device(&format!(r"\\.\{letter}:"), 0)?;
        let number = ioctl::device_io_control(
            &handle,
            IOCTL_STORAGE_GET_DEVICE_NUMBER,
            &[],
            size_of::<STORAGE_DEVICE_NUMBER>(),
        )?;
        let disk_number = dword(&number, offset_of!(STORAGE_DEVICE_NUMBER, DeviceNumber))?;
        let extents =
            ioctl::device_io_control(&handle, IOCTL_VOLUME_GET_VOLUME_DISK_EXTENTS, &[], 4096)?;
        if single_extent_disk(&extents)? != Some(disk_number) {
            return Err(Error::msg(
                "volume mapping",
                "volume spans disks or has an ambiguous device number; WMI associations required",
            ));
        }
        let mut serial = 0;
        // SAFETY: root stays live and serial is the only writable output requested.
        unsafe {
            GetVolumeInformationW(
                PCWSTR(root.as_ptr()),
                None,
                Some(&mut serial),
                None,
                None,
                None,
            )
        }
        .map_err(|e| Error::from_win("GetVolumeInformationW", e))?;
        Ok(Some(Volume {
            disk_number,
            serial: format!("{serial:08X}"),
        }))
    })
}

/// Reads the selected VPD identifier through the shared device/IOCTL helpers.
pub fn physical_identifier(index: u32) -> Result<Option<Identifier>> {
    bounded("StorageDeviceIdProperty", move || {
        // C# parity: Services/Win32/StorageDeviceIdQuery.cs:202-211 (read access).
        let handle = ioctl::open_device(&format!(r"\\.\PHYSICALDRIVE{index}"), GENERIC_READ.0)?;
        // Include the SDK AdditionalParameters tail, with zero-initialized padding.
        let mut query = vec![0; size_of::<STORAGE_PROPERTY_QUERY>()];
        let offset = offset_of!(STORAGE_PROPERTY_QUERY, PropertyId);
        query[offset..offset + 4].copy_from_slice(&StorageDeviceIdProperty.0.to_le_bytes());
        let offset = offset_of!(STORAGE_PROPERTY_QUERY, QueryType);
        query[offset..offset + 4].copy_from_slice(&PropertyStandardQuery.0.to_le_bytes());
        parse_descriptor(&ioctl::device_io_control(
            &handle,
            IOCTL_STORAGE_QUERY_PROPERTY,
            &query,
            4096,
        )?)
    })
}

/// Reads GPT/MBR identity through the shared device/IOCTL helpers.
pub fn physical_layout(index: u32) -> Result<Option<DiskLayout>> {
    bounded("IOCTL_DISK_GET_DRIVE_LAYOUT_EX", move || {
        // C# parity: Services/Win32/StorageDeviceIdQuery.cs:354-363.
        let handle = ioctl::open_device(&format!(r"\\.\PHYSICALDRIVE{index}"), GENERIC_READ.0)?;
        parse_layout(&ioctl::device_io_control(
            &handle,
            IOCTL_DISK_GET_DRIVE_LAYOUT_EX,
            &[],
            4096,
        )?)
    })
}

fn bytes<const N: usize>(data: &[u8], offset: usize) -> Result<[u8; N]> {
    let end = offset
        .checked_add(N)
        .ok_or_else(|| Error::msg("storage parser", "offset overflow"))?;
    data.get(offset..end)
        .and_then(|b| b.try_into().ok())
        .ok_or_else(|| Error::msg("storage parser", "truncated buffer"))
}

fn dword(data: &[u8], offset: usize) -> Result<u32> {
    Ok(u32::from_le_bytes(bytes(data, offset)?))
}
fn word(data: &[u8], offset: usize) -> Result<u16> {
    Ok(u16::from_le_bytes(bytes(data, offset)?))
}

fn parse_descriptor(data: &[u8]) -> Result<Option<Identifier>> {
    const HEADER: usize = offset_of!(STORAGE_DEVICE_ID_DESCRIPTOR, Identifiers);
    const PAYLOAD: usize = offset_of!(STORAGE_IDENTIFIER, Identifier);
    let size = dword(data, offset_of!(STORAGE_DEVICE_ID_DESCRIPTOR, Size))? as usize;
    if size < HEADER || size > data.len() {
        return Err(Error::msg("storage descriptor", "invalid descriptor size"));
    }
    let data = &data[..size];
    let count = dword(
        data,
        offset_of!(STORAGE_DEVICE_ID_DESCRIPTOR, NumberOfIdentifiers),
    )? as usize;
    if count > (size - HEADER) / PAYLOAD {
        return Err(Error::msg(
            "storage descriptor",
            "identifier count exceeds buffer",
        ));
    }
    let mut offset = HEADER;
    let mut best = None;
    let mut best_rank = 3;
    for i in 0..count {
        let header = data
            .get(offset..offset + PAYLOAD)
            .ok_or_else(|| Error::msg("storage descriptor", "truncated identifier header"))?;
        let code = dword(header, offset_of!(STORAGE_IDENTIFIER, CodeSet))?;
        let kind = dword(header, offset_of!(STORAGE_IDENTIFIER, Type))?;
        let length = word(header, offset_of!(STORAGE_IDENTIFIER, IdentifierSize))? as usize;
        let next = word(header, offset_of!(STORAGE_IDENTIFIER, NextOffset))? as usize;
        let end = offset + PAYLOAD + length;
        let value = data.get(offset + PAYLOAD..end).ok_or_else(|| {
            Error::msg("storage descriptor", "identifier size exceeds descriptor")
        })?;
        // C# parity: Services/Win32/StorageDeviceIdQuery.cs:519-540. Association
        // does not filter selection: first ASCII, then nonzero binary 8/16, first.
        let rank = if code == StorageIdCodeSetAscii.0 as u32
            && (kind == StorageIdTypeScsiNameString.0 as u32
                || kind == StorageIdTypeVendorId.0 as u32
                || length > 0)
        {
            0
        } else if code == StorageIdCodeSetBinary.0 as u32
            && matches!(length, 8 | 16)
            && value.iter().any(|&b| b != 0)
        {
            1
        } else {
            2
        };
        if rank < best_rank {
            best = Some((code, value));
            best_rank = rank;
        }
        // Real drivers set the last NextOffset to its aligned size, which may point
        // at or past Size. C# parity: Services/Win32/StorageDeviceIdQuery.cs:541-545
        // (stop at count or a zero NextOffset and keep the best identifier so far).
        if i + 1 == count || next == 0 {
            break;
        }
        if next < PAYLOAD + length || offset + next > size {
            return Err(Error::msg(
                "storage descriptor",
                "invalid next identifier offset",
            ));
        }
        offset += next;
    }
    Ok(best.map(|(code, value)| {
        // C# parity: Services/Win32/StorageDeviceIdQuery.cs:306-311,595-609.
        // ASCII decoding replaces high bytes with '?' and trims only trailing NUL.
        let printable =
            !value.is_empty() && value.iter().all(|&b| b == 0 || (32..=126).contains(&b));
        let decoded = if code == StorageIdCodeSetAscii.0 as u32 || printable {
            value
                .iter()
                .map(|&b| if b.is_ascii() { char::from(b) } else { '?' })
                .collect::<String>()
                .trim_end_matches('\0')
                .to_owned()
        } else {
            String::new()
        };
        Identifier {
            hex: colon_hex(value),
            decoded,
        }
    }))
}

fn parse_layout(data: &[u8]) -> Result<Option<DiskLayout>> {
    const HEADER: usize = offset_of!(DRIVE_LAYOUT_INFORMATION_EX, PartitionEntry);
    const UNION: usize = offset_of!(DRIVE_LAYOUT_INFORMATION_EX, Anonymous);
    if data.len() < HEADER {
        return Err(Error::msg("drive layout", "truncated layout header"));
    }
    let style = dword(
        data,
        offset_of!(DRIVE_LAYOUT_INFORMATION_EX, PartitionStyle),
    )?;
    // C# parity: Services/Win32/StorageDeviceIdQuery.cs:460-469 (RAW has no IDs).
    if style != PARTITION_STYLE_GPT.0 as u32 && style != PARTITION_STYLE_MBR.0 as u32 {
        return Ok(None);
    }
    let count = dword(
        data,
        offset_of!(DRIVE_LAYOUT_INFORMATION_EX, PartitionCount),
    )? as usize;
    if count > (data.len() - HEADER) / size_of::<PARTITION_INFORMATION_EX>() {
        return Err(Error::msg(
            "drive layout",
            "partition count exceeds returned buffer",
        ));
    }
    let is_gpt = style == PARTITION_STYLE_GPT.0 as u32;
    let disk_id = if is_gpt {
        guid_text(bytes(
            data,
            UNION + offset_of!(DRIVE_LAYOUT_INFORMATION_GPT, DiskId),
        )?)
    } else {
        format!(
            "0x{:08X}",
            dword(
                data,
                UNION + offset_of!(DRIVE_LAYOUT_INFORMATION_MBR, Signature)
            )?
        )
    };
    let mut partition_guids = Vec::new();
    if is_gpt {
        for i in 0..count {
            let offset = HEADER + i * size_of::<PARTITION_INFORMATION_EX>();
            // C# parity: Services/Win32/StorageDeviceIdQuery.cs:438-457.
            if dword(
                data,
                offset + offset_of!(PARTITION_INFORMATION_EX, PartitionStyle),
            )? == PARTITION_STYLE_GPT.0 as u32
            {
                let guid = bytes(
                    data,
                    offset
                        + offset_of!(PARTITION_INFORMATION_EX, Anonymous)
                        + offset_of!(PARTITION_INFORMATION_GPT, PartitionId),
                )?;
                if guid != [0; 16] {
                    partition_guids.push(guid_text(guid));
                }
            }
        }
    }
    Ok(Some(DiskLayout {
        is_gpt,
        disk_id,
        partition_guids,
    }))
}

fn single_extent_disk(data: &[u8]) -> Result<Option<u32>> {
    const HEADER: usize = offset_of!(VOLUME_DISK_EXTENTS, Extents);
    let count = dword(data, offset_of!(VOLUME_DISK_EXTENTS, NumberOfDiskExtents))? as usize;
    if data.len() < HEADER || count > (data.len() - HEADER) / size_of::<DISK_EXTENT>() {
        return Err(Error::msg("volume extents", "extent count exceeds buffer"));
    }
    let mut disk = None;
    for i in 0..count {
        let number = dword(
            data,
            HEADER + i * size_of::<DISK_EXTENT>() + offset_of!(DISK_EXTENT, DiskNumber),
        )?;
        if disk.is_some_and(|old| old != number) {
            return Ok(None);
        }
        disk = Some(number);
    }
    Ok(disk)
}

/// Formats bytes as the C# uppercase, colon-separated hexadecimal representation.
pub fn colon_hex(bytes: &[u8]) -> String {
    bytes
        .iter()
        .map(|b| format!("{b:02X}"))
        .collect::<Vec<_>>()
        .join(":")
}

fn guid_text(bytes: [u8; 16]) -> String {
    // C# parity: Hardware/DiskDriveInfo.cs:185-191 (Guid D, uppercase).
    format!(
        "{:08X}-{:04X}-{:04X}-{:02X}{:02X}-{:02X}{:02X}{:02X}{:02X}{:02X}{:02X}",
        u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]),
        u16::from_le_bytes([bytes[4], bytes[5]]),
        u16::from_le_bytes([bytes[6], bytes[7]]),
        bytes[8],
        bytes[9],
        bytes[10],
        bytes[11],
        bytes[12],
        bytes[13],
        bytes[14],
        bytes[15]
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    fn fixture_bytes(case: &Value) -> Vec<u8> {
        let hex = case["hex"].as_str().expect("fixture hex");
        (0..hex.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).expect("hex byte"))
            .collect()
    }

    #[test]
    fn storage_descriptor_fixtures() {
        let fixture: Value =
            serde_json::from_str(include_str!("../../tests/fixtures/wp-01/storage.json"))
                .expect("fixture JSON");
        assert_eq!(offset_of!(STORAGE_DEVICE_ID_DESCRIPTOR, Identifiers), 12);
        assert_eq!(offset_of!(STORAGE_IDENTIFIER, Identifier), 16);
        for case in fixture["descriptors"].as_array().expect("descriptor cases") {
            let data = fixture_bytes(case);
            let result = parse_descriptor(&data);
            if case["error"] == true {
                assert!(result.is_err(), "{}", case["name"]);
                continue;
            }
            let result = result.expect("valid descriptor");
            if case["expected"].is_null() {
                assert!(result.is_none());
            } else {
                let id = result.expect("identifier");
                assert_eq!(id.hex, case["expected"]["hex"], "{}", case["name"]);
                assert_eq!(id.decoded, case["expected"]["decoded"], "{}", case["name"]);
            }
            // Every prefix is truncated, even when its first identifier was valid.
            for end in 0..data.len() {
                assert!(
                    parse_descriptor(&data[..end]).is_err(),
                    "{} prefix {end}",
                    case["name"]
                );
            }
        }
    }

    #[test]
    fn drive_layout_and_extent_fixtures() {
        let fixture: Value =
            serde_json::from_str(include_str!("../../tests/fixtures/wp-01/storage.json"))
                .expect("fixture JSON");
        assert_eq!(offset_of!(DRIVE_LAYOUT_INFORMATION_EX, PartitionEntry), 48);
        assert_eq!(size_of::<PARTITION_INFORMATION_EX>(), 144);
        assert_eq!(
            offset_of!(PARTITION_INFORMATION_EX, Anonymous)
                + offset_of!(PARTITION_INFORMATION_GPT, PartitionId),
            48
        );
        for case in fixture["layouts"].as_array().expect("layout cases") {
            let data = fixture_bytes(case);
            let result = parse_layout(&data);
            if case["error"] == true {
                assert!(result.is_err(), "{}", case["name"]);
                continue;
            }
            let result = result.expect("valid layout");
            if case["expected"].is_null() {
                assert!(result.is_none());
            } else {
                let layout = result.expect("GPT/MBR");
                assert_eq!(layout.is_gpt, case["expected"]["is_gpt"]);
                assert_eq!(layout.disk_id, case["expected"]["disk_id"]);
                assert_eq!(
                    serde_json::to_value(layout.partition_guids).expect("GUID array"),
                    case["expected"]["partition_guids"]
                );
                for end in 0..data.len() {
                    assert!(
                        parse_layout(&data[..end]).is_err(),
                        "{} prefix {end}",
                        case["name"]
                    );
                }
            }
        }
        for case in fixture["extents"].as_array().expect("extent cases") {
            let result = single_extent_disk(&fixture_bytes(case));
            if case["error"] == true {
                assert!(result.is_err(), "{}", case["name"]);
            } else {
                assert_eq!(
                    result.expect("valid extents").map(u64::from),
                    case["expected"].as_u64(),
                    "{}",
                    case["name"]
                );
            }
        }
    }
}
