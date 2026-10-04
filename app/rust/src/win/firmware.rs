//! Raw firmware tables and bounds-checked SMBIOS access.

use super::{Error, Result, record};
use windows::Win32::System::SystemInformation::{FIRMWARE_TABLE_PROVIDER, GetSystemFirmwareTable};

#[derive(Clone, Debug)]
pub struct Smbios {
    pub major: u8,
    pub minor: u8,
    pub structures: Vec<Structure>,
}
#[derive(Clone, Debug)]
pub struct Structure {
    pub kind: u8,
    pub handle: u16,
    pub formatted: Vec<u8>,
    pub strings: Vec<String>,
}

/// Reads a firmware table using the numeric provider signature and table ID.
pub fn raw_table(provider: u32, id: u32) -> Result<Vec<u8>> {
    let provider = FIRMWARE_TABLE_PROVIDER(provider);
    // SAFETY: A null output buffer requests the size without writing any memory.
    let mut size = unsafe { GetSystemFirmwareTable(provider, id, None) };
    if size == 0 {
        return Err(Error::last("GetSystemFirmwareTable"));
    }
    // A changing firmware table must not cause an unbounded allocation/retry loop.
    for _ in 0..3 {
        let mut bytes = Vec::new();
        bytes
            .try_reserve_exact(size as usize)
            .map_err(|e| Error::msg("GetSystemFirmwareTable", e.to_string()))?;
        bytes.resize(size as usize, 0);
        // SAFETY: The writable slice has the queried size, which fits the SDK's u32 count.
        let written = unsafe { GetSystemFirmwareTable(provider, id, Some(&mut bytes)) };
        if written == 0 {
            return Err(Error::last("GetSystemFirmwareTable"));
        }
        if written <= size {
            bytes.truncate(written as usize);
            return Ok(bytes);
        }
        size = written;
    }
    Err(Error::msg(
        "GetSystemFirmwareTable",
        "table kept growing during three reads",
    ))
}

/// Parses a complete RawSMBIOSData buffer including its eight-byte header.
pub fn parse_smbios(raw: &[u8]) -> Result<Smbios> {
    let header = raw
        .get(..8)
        .ok_or_else(|| Error::msg("SMBIOS parse", "truncated RawSMBIOSData header"))?;
    let length = u32::from_le_bytes([header[4], header[5], header[6], header[7]]) as usize;
    let end = 8_usize
        .checked_add(length)
        .ok_or_else(|| Error::msg("SMBIOS parse", "table length overflow"))?;
    // C# parity: app/src/Services/Win32/FirmwareTable.cs:69-71
    if length == 0 || end > raw.len() {
        return Err(Error::msg("SMBIOS parse", "invalid SMBIOS table length"));
    }
    let table = &raw[8..end];
    let mut result = Smbios {
        major: header[1],
        minor: header[2],
        structures: Vec::new(),
    };
    let mut offset = 0;
    while offset < table.len() {
        let Some(header) = table.get(offset..).and_then(|tail| tail.get(..4)) else {
            record(Error::msg(
                "SMBIOS parse",
                "truncated final structure header",
            ));
            break;
        };
        let length = header[1] as usize;
        // C# parity: app/src/Services/Win32/FirmwareTable.cs:115-117
        // Preserve complete earlier structures when the final formatted area is truncated.
        if length < 4 || length > table.len() - offset {
            record(Error::msg(
                "SMBIOS parse",
                "invalid or truncated final formatted structure",
            ));
            break;
        }
        // C# parity: app/src/Services/Win32/FirmwareTable.cs:136-137
        if header[0] == 127 {
            break;
        }
        let formatted_end = offset + length;
        let strings_tail = &table[formatted_end..];
        // F4: consume BOTH terminating NULs, including a zero-string structure.
        let terminator = strings_tail.windows(2).position(|pair| pair == [0, 0]);
        let strings_end = terminator.unwrap_or(strings_tail.len());
        let strings = strings_tail[..strings_end]
            .split(|byte| *byte == 0)
            .filter(|string| !string.is_empty())
            .map(|string| {
                // C# parity: app/src/Services/Win32/FirmwareTable.cs:173
                // .NET ASCII decoding replaces each non-ASCII byte with '?'.
                string
                    .iter()
                    .map(|byte| {
                        if byte.is_ascii() {
                            char::from(*byte)
                        } else {
                            '?'
                        }
                    })
                    .collect()
            })
            .collect();
        result.structures.push(Structure {
            kind: header[0],
            handle: u16::from_le_bytes([header[2], header[3]]),
            formatted: table[offset..formatted_end].to_vec(),
            strings,
        });
        // C# parity: app/src/Services/Win32/FirmwareTable.cs:166-181
        // An unterminated final string table still contributes its available strings.
        match terminator {
            Some(end) => offset = formatted_end + end + 2,
            None => {
                record(Error::msg(
                    "SMBIOS parse",
                    "unterminated final string table",
                ));
                break;
            }
        }
    }
    Ok(result)
}
/// Reads and parses the RSMB table.
pub fn smbios() -> Result<Smbios> {
    // C# parity: app/src/Services/Win32/FirmwareTable.cs:13
    parse_smbios(&raw_table(0x5253_4D42, 0)?)
}

impl Structure {
    /// Reads a one-based SMBIOS string index; zero or missing returns an empty string.
    pub fn string(&self, idx: u8) -> &str {
        // C# parity: app/src/Services/Win32/FirmwareTable.cs:184-189
        idx.checked_sub(1)
            .and_then(|index| self.strings.get(index as usize))
            .map_or("", String::as_str)
    }
    /// Reads a byte at an absolute formatted-structure offset, or None if truncated.
    pub fn byte(&self, offset: usize) -> Option<u8> {
        self.formatted.get(offset).copied()
    }
    /// Reads a little-endian word at an absolute formatted-structure offset.
    pub fn word(&self, offset: usize) -> Option<u16> {
        self.read_bytes(offset).map(u16::from_le_bytes)
    }
    /// Reads a little-endian dword at an absolute formatted-structure offset.
    pub fn dword(&self, offset: usize) -> Option<u32> {
        self.read_bytes(offset).map(u32::from_le_bytes)
    }
    /// Reads a little-endian qword at an absolute formatted-structure offset.
    pub fn qword(&self, offset: usize) -> Option<u64> {
        self.read_bytes(offset).map(u64::from_le_bytes)
    }

    fn read_bytes<const N: usize>(&self, offset: usize) -> Option<[u8; N]> {
        self.formatted
            .get(offset..offset.checked_add(N)?)?
            .try_into()
            .ok()
    }
}

/// Identity fields from one type-22 record; each decode preserves sibling fields.
pub struct PortableBattery {
    pub name: Result<Option<String>>,
    pub manufacturer: Result<Option<String>>,
    pub date: Result<Option<String>>,
    pub serial: Result<Option<String>>,
}

/// Decodes only portable batteries from the shared RSMB snapshot, in table order.
pub fn portable_batteries(table: &Smbios) -> Vec<PortableBattery> {
    table
        .structures
        .iter()
        .filter(|record| record.kind == 22)
        .map(|record| {
            let string = |offset| battery_string(record, offset);
            let sbds = (table.major, table.minor) >= (2, 2);
            let serial = if sbds && record.byte(7) == Some(0) {
                // DSP0134 7.23: SBDS serial is valid only when the string index is zero.
                // Zero is a valid 16-bit serial; format four hex digits so it is maskable.
                record
                    .word(0x10)
                    .map(|value| Some(format!("{value:04X}")))
                    .ok_or_else(|| Error::msg("SMBIOS battery", "truncated SBDS serial"))
            } else {
                string(7)
            };
            let date = if sbds && record.byte(6) == Some(0) {
                match record.word(0x12) {
                    None => Err(Error::msg("SMBIOS battery", "truncated SBDS date")),
                    Some(0) => Ok(None),
                    Some(value) => battery_date(
                        1980 + (value >> 9),
                        ((value >> 5) & 15) as u8,
                        (value & 31) as u8,
                    )
                    .map(Some),
                }
            } else {
                string(6)
                    .and_then(|value| value.map(|value| battery_date_string(&value)).transpose())
            };
            PortableBattery {
                name: string(8),
                manufacturer: string(5),
                date,
                serial,
            }
        })
        .collect()
}

fn battery_string(record: &Structure, offset: usize) -> Result<Option<String>> {
    let index = record
        .byte(offset)
        .ok_or_else(|| Error::msg("SMBIOS battery", "truncated string index"))?;
    if index == 0 {
        return Ok(None);
    }
    let value = record
        .strings
        .get(usize::from(index) - 1)
        .ok_or_else(|| Error::msg("SMBIOS battery", "string index outside string table"))?;
    if value.contains('?') || value.chars().any(char::is_control) {
        return Err(Error::msg(
            "SMBIOS battery",
            "malformed: control or unverified ASCII replacement in string",
        ));
    }
    if matches!(
        value.trim().to_ascii_lowercase().as_str(),
        "unknown"
            | "none"
            | "n/a"
            | "default string"
            | "to be filled by o.e.m."
            | "to be filled by oem"
    ) {
        return Err(Error::msg(
            "SMBIOS battery",
            "placeholder: battery field omitted",
        ));
    }
    if offset == 7
        && !value.is_empty()
        && value.chars().all(|character| value.starts_with(character))
    {
        return Err(Error::msg(
            "SMBIOS battery",
            "implausible: repeated-character serial",
        ));
    }
    Ok((!value.is_empty()).then(|| value.clone()))
}

/// Checks the calendar date used by battery IOCTL and SBDS encodings.
pub fn battery_date(year: u16, month: u8, day: u8) -> Result<String> {
    // SAFETY: GetLocalTime returns initialized calendar storage and has no inputs.
    let today = unsafe { windows::Win32::System::SystemInformation::GetLocalTime() };
    battery_date_at(year, month, day, (today.wYear, today.wMonth, today.wDay))
}

fn battery_date_at(year: u16, month: u8, day: u8, today: (u16, u16, u16)) -> Result<String> {
    let leap = year.is_multiple_of(4) && (!year.is_multiple_of(100) || year.is_multiple_of(400));
    let days = match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if leap => 29,
        2 => 28,
        _ => 0,
    };
    if year == 0 || day == 0 || day > days {
        return Err(Error::msg("battery date", "malformed calendar date"));
    }
    if (year, u16::from(month), u16::from(day)) > today {
        return Err(Error::msg(
            "battery date",
            "implausible future manufacture date",
        ));
    }
    Ok(format!("{year:04}-{month:02}-{day:02}"))
}

fn battery_date_string(value: &str) -> Result<String> {
    let value = value.trim();
    let parts: Vec<_> = value.split(['-', '/']).collect();
    let parsed = match parts.as_slice() {
        [year, month, day] if year.len() == 4 => year
            .parse::<u16>()
            .ok()
            .zip(month.parse::<u8>().ok())
            .zip(day.parse::<u8>().ok())
            .map(|((year, month), day)| (year, month, day)),
        [month, day, year] if year.len() == 4 => year
            .parse::<u16>()
            .ok()
            .zip(month.parse::<u8>().ok())
            .zip(day.parse::<u8>().ok())
            .map(|((year, month), day)| (year, month, day)),
        _ => None,
    }
    .ok_or_else(|| Error::msg("SMBIOS battery date", "implausible unknown date encoding"))?;
    battery_date(parsed.0, parsed.1, parsed.2)?;
    Ok(value.to_owned())
}

/// Per-socket context from type 4, separate from the legacy processor identity.
pub struct ProcessorMetadata {
    pub handle: u16,
    pub socket: Result<Option<String>>,
    pub manufacturer: Result<Option<String>>,
    pub part: Result<Option<String>>,
    pub asset: Result<Option<String>>,
}

pub fn processor_metadata(table: &Smbios) -> Vec<ProcessorMetadata> {
    table
        .structures
        .iter()
        .filter(|record| record.kind == 4)
        .map(|record| {
            // Asset/part fields were introduced in SMBIOS 2.3.
            let optional = |offset| {
                if (table.major, table.minor) < (2, 3) {
                    Ok(None)
                } else {
                    metadata_string(record, offset)
                }
            };
            ProcessorMetadata {
                handle: record.handle,
                socket: metadata_string(record, 4),
                manufacturer: metadata_string(record, 7),
                part: optional(0x22),
                asset: optional(0x21),
            }
        })
        .collect()
}

/// Each type-39 record retains independently decoded identity/context fields.
pub struct PowerSupply {
    pub manufacturer: Result<Option<String>>,
    pub model: Result<Option<String>>,
    pub revision: Result<Option<String>>,
    pub serial: Result<Option<String>>,
    pub asset: Result<Option<String>>,
}

pub fn power_supplies(table: &Smbios) -> Vec<PowerSupply> {
    table
        .structures
        .iter()
        .filter(|record| record.kind == 39)
        .map(|record| PowerSupply {
            manufacturer: metadata_string(record, 7),
            model: metadata_string(record, 0x0A),
            revision: metadata_string(record, 0x0B),
            serial: metadata_string(record, 8),
            asset: metadata_string(record, 9),
        })
        .collect()
}

/// Firmware-published TPM context; none of these fields is a per-unit identity.
pub struct TpmDevice {
    pub handle: u16,
    pub vendor: Result<Option<String>>,
    pub spec: Result<(u8, u8)>,
    pub firmware1: Result<u32>,
    pub firmware2: Result<u32>,
    pub description: Result<Option<String>>,
    pub characteristics: Result<u64>,
}

pub fn tpm_devices(table: &Smbios) -> Vec<TpmDevice> {
    table
        .structures
        .iter()
        .filter(|record| record.kind == 43)
        .map(|record| TpmDevice {
            handle: record.handle,
            vendor: tpm_vendor(record),
            spec: record
                .byte(8)
                .zip(record.byte(9))
                .ok_or_else(|| metadata_error("truncated TPM specification version"))
                .and_then(|version| {
                    if !matches!(version, (1, 2) | (2, 0)) {
                        Err(metadata_error("invalid TPM specification version"))
                    } else {
                        Ok(version)
                    }
                }),
            firmware1: record
                .dword(0x0A)
                .ok_or_else(|| metadata_error("truncated TPM firmware version 1")),
            firmware2: record
                .dword(0x0E)
                .ok_or_else(|| metadata_error("truncated TPM firmware version 2")),
            description: metadata_string(record, 0x12),
            characteristics: record
                .qword(0x13)
                .ok_or_else(|| metadata_error("truncated TPM characteristics")),
        })
        .collect()
}

fn tpm_vendor(record: &Structure) -> Result<Option<String>> {
    let bytes = record
        .formatted
        .get(4..8)
        .ok_or_else(|| metadata_error("truncated TPM vendor ID"))?;
    let end = bytes.iter().position(|byte| *byte == 0).unwrap_or(4);
    let vendor = bytes
        .get(..end)
        .ok_or_else(|| metadata_error("malformed TPM vendor bounds"))?;
    let padding = bytes
        .get(end..)
        .ok_or_else(|| metadata_error("malformed TPM padding bounds"))?;
    if vendor
        .iter()
        .any(|byte| !byte.is_ascii_graphic() && *byte != b' ')
        || padding.iter().any(|byte| *byte != 0)
    {
        return Err(metadata_error("malformed TPM vendor ID"));
    }
    let value: String = vendor.iter().copied().map(char::from).collect();
    Ok((!value.trim().is_empty()).then(|| value.trim().to_owned()))
}

/// Type-45 values describe firmware components, not the machine's identity.
pub struct FirmwareComponent {
    pub handle: u16,
    pub name: Result<Option<String>>,
    pub version: Result<Option<String>>,
    pub id: Result<Option<String>>,
    pub date: Result<Option<String>>,
}

pub fn firmware_components(table: &Smbios) -> Vec<FirmwareComponent> {
    table
        .structures
        .iter()
        .filter(|record| record.kind == 45)
        .map(|record| {
            // Association handles are context only; never guess a target device.
            if let Some(count) = record.byte(0x17).filter(|count| *count != 0) {
                let end = 0x18 + usize::from(count) * 2;
                if record.formatted.get(0x18..end).is_none() {
                    record_association_failure(
                        "malformed: truncated component association handles",
                    );
                } else {
                    record_association_failure(
                        "unsupported: component handles not joined to devices",
                    );
                }
            }
            FirmwareComponent {
                handle: record.handle,
                name: metadata_string(record, 4),
                version: metadata_string(record, 5),
                id: metadata_string(record, 7),
                date: metadata_string(record, 9),
            }
        })
        .collect()
}

fn record_association_failure(detail: &'static str) {
    record(Error::msg("SMBIOS type 45 associations", detail));
}

fn metadata_error(detail: &'static str) -> Error {
    Error::msg("SMBIOS metadata", format!("malformed: {detail}"))
}

fn metadata_string(record: &Structure, offset: usize) -> Result<Option<String>> {
    let index = record
        .byte(offset)
        .ok_or_else(|| metadata_error("truncated string index"))?;
    if index == 0 {
        return Ok(None);
    }
    let value = record
        .strings
        .get(usize::from(index) - 1)
        .ok_or_else(|| metadata_error("string index outside string table"))?;
    if value.contains('?') || value.chars().any(char::is_control) {
        return Err(metadata_error(
            "control or unverified ASCII replacement in string",
        ));
    }
    Ok((!value.trim().is_empty()).then(|| value.trim().to_owned()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn battery_future_dates_and_component_associations_leave_siblings_intact() {
        assert!(
            battery_date_at(2026, 10, 5, (2026, 10, 4))
                .unwrap_err()
                .detail
                .contains("implausible")
        );
        assert!(battery_date_at(2026, 4, 31, (2026, 10, 4)).is_err());
        assert_eq!(
            battery_date_at(2024, 2, 29, (2026, 10, 4)).unwrap(),
            "2024-02-29"
        );
        assert!(battery_date_string("not-a-date").is_err());
        let mut record = Structure {
            kind: 45,
            handle: 0x4518,
            formatted: vec![0; 0x1C],
            strings: vec!["System Firmware".into()],
        };
        record.formatted[4] = 1;
        record.formatted[0x17] = 2;
        record.formatted[0x18..].copy_from_slice(&[0x01, 0x04, 0x01, 0x39]);
        let mut table = Smbios {
            major: 3,
            minor: 6,
            structures: vec![record],
        };
        for end in [0x1C, 0x19] {
            table.structures[0].formatted.truncate(end);
            let component = firmware_components(&table).remove(0);
            assert_eq!(component.name.unwrap().as_deref(), Some("System Firmware"));
            assert!(component.version.unwrap().is_none());
            assert!(component.id.unwrap().is_none());
        }
        let failures = super::super::take_recorded();
        assert!(
            failures
                .iter()
                .any(|failure| failure.op == "SMBIOS type 45 associations"
                    && failure.detail.contains("malformed"))
        );
        assert!(
            failures
                .iter()
                .any(|failure| failure.op == "SMBIOS type 45 associations"
                    && failure.detail.contains("unsupported"))
        );
        assert!(
            failures
                .iter()
                .all(|failure| !failure.detail.contains("System Firmware"))
        );
    }

    const ZERO_STRINGS: &str = include_str!("../../tests/fixtures/phase1-smbios/zero-strings.hex");
    const TRUNCATED: &str = include_str!("../../tests/fixtures/phase1-smbios/truncated-last.hex");
    const REPEATED: &str = include_str!("../../tests/fixtures/phase1-smbios/repeated-types.hex");
    const INDEXES: &str = include_str!("../../tests/fixtures/phase1-smbios/string-indexes.hex");

    #[test]
    fn type4_metadata_keeps_handles_optional_version_fields_and_partial_strings() {
        let mut record = Structure {
            kind: 4,
            handle: 0x0401,
            formatted: vec![0; 0x23],
            strings: [
                "CPU1",
                "Intel(R) Corporation",
                "BX8071512700K",
                "CPU-INV-2418",
            ]
            .map(String::from)
            .to_vec(),
        };
        for (offset, index) in [(4, 1), (7, 2), (0x22, 3), (0x21, 4)] {
            record.formatted[offset] = index;
        }
        let mut table = Smbios {
            major: 3,
            minor: 6,
            structures: vec![record.clone()],
        };
        let first = processor_metadata(&table).remove(0);
        assert_eq!(first.handle, 0x0401);
        assert_eq!(first.socket.unwrap().as_deref(), Some("CPU1"));
        assert_eq!(
            first.manufacturer.unwrap().as_deref(),
            Some("Intel(R) Corporation")
        );
        assert_eq!(first.part.unwrap().as_deref(), Some("BX8071512700K"));
        assert_eq!(first.asset.unwrap().as_deref(), Some("CPU-INV-2418"));
        record.handle = 0x0402;
        record.formatted[0x21] = 255;
        table.structures.push(record.clone());
        let records = processor_metadata(&table);
        assert_eq!(
            records.iter().map(|r| r.handle).collect::<Vec<_>>(),
            [0x0401, 0x0402]
        );
        assert!(records[1].asset.is_err());
        assert!(records[1].part.is_ok());
        for end in 0..0x23 {
            record.formatted.truncate(end);
            let mut short = table.clone();
            short.structures = vec![record.clone()];
            let value = processor_metadata(&short).remove(0);
            assert_eq!(value.socket.is_ok(), end > 4);
            assert_eq!(value.manufacturer.is_ok(), end > 7);
            assert!(value.part.is_err());
            record = table.structures[0].clone();
        }
        table.major = 2;
        table.minor = 2;
        table.structures[0].formatted.truncate(0x20);
        let old = processor_metadata(&table).remove(0);
        assert!(old.part.unwrap().is_none());
        assert!(old.asset.unwrap().is_none());
        assert!(old.socket.unwrap().is_some());
    }

    #[test]
    fn type39_power_supply_offsets_bounds_and_bad_strings_are_independent() {
        let mut record = Structure {
            kind: 39,
            handle: 0x3901,
            formatted: vec![0; 0x16],
            strings: [
                "Delta Electronics",
                "DPS-750AB-12",
                "A01",
                "PSU2418K73196",
                "INV-PSU-2418",
            ]
            .map(String::from)
            .to_vec(),
        };
        record.formatted[7..12].copy_from_slice(&[1, 4, 5, 2, 3]);
        let table = |record: Structure| Smbios {
            major: 3,
            minor: 6,
            structures: vec![record],
        };
        let supply = power_supplies(&table(record.clone())).remove(0);
        let mut repeated = table(record.clone());
        repeated.structures.push(record.clone());
        assert_eq!(power_supplies(&repeated).len(), 2);
        assert_eq!(
            supply.manufacturer.unwrap().as_deref(),
            Some("Delta Electronics")
        );
        assert_eq!(supply.model.unwrap().as_deref(), Some("DPS-750AB-12"));
        assert_eq!(supply.revision.unwrap().as_deref(), Some("A01"));
        assert_eq!(supply.serial.unwrap().as_deref(), Some("PSU2418K73196"));
        assert_eq!(supply.asset.unwrap().as_deref(), Some("INV-PSU-2418"));
        for end in 0..12 {
            let mut short = record.clone();
            short.formatted.truncate(end);
            let supply = power_supplies(&table(short)).remove(0);
            for (offset, field) in [
                (7, supply.manufacturer),
                (8, supply.serial),
                (9, supply.asset),
                (10, supply.model),
                (11, supply.revision),
            ] {
                assert_eq!(field.is_ok(), end > offset);
            }
        }
        record.formatted[8] = 255;
        record.formatted[9] = 0;
        record.strings[1] = "DPS\r\n750".into();
        let supply = power_supplies(&table(record)).remove(0);
        assert!(supply.serial.is_err());
        assert!(supply.asset.unwrap().is_none());
        assert!(supply.model.is_err());
        assert!(supply.manufacturer.is_ok());
        assert!(supply.revision.is_ok());
    }

    #[test]
    fn type43_tpm_vendor_version_words_and_characteristics_are_bounds_checked() {
        let mut record = Structure {
            kind: 43,
            handle: 0x4301,
            formatted: vec![0; 0x1F],
            strings: vec!["Firmware TPM".into()],
        };
        record.formatted[4..10].copy_from_slice(&[b'I', b'F', b'X', 0, 2, 0]);
        record.formatted[0x0A..0x0E].copy_from_slice(&0x0007_0055_u32.to_le_bytes());
        record.formatted[0x0E..0x12].copy_from_slice(&0x11CB_0000_u32.to_le_bytes());
        record.formatted[0x12] = 1;
        record.formatted[0x13..0x1B].copy_from_slice(&0x8000_0000_0000_0020_u64.to_le_bytes());
        let table = |record: Structure| Smbios {
            major: 3,
            minor: 6,
            structures: vec![record],
        };
        let device = tpm_devices(&table(record.clone())).remove(0);
        let mut repeated = table(record.clone());
        let mut second = record.clone();
        second.handle = 0x4302;
        repeated.structures.push(second);
        assert_eq!(
            tpm_devices(&repeated)
                .iter()
                .map(|device| device.handle)
                .collect::<Vec<_>>(),
            [0x4301, 0x4302]
        );
        assert_eq!(device.handle, 0x4301);
        assert_eq!(device.vendor.unwrap().as_deref(), Some("IFX"));
        assert_eq!(device.spec.unwrap(), (2, 0));
        assert_eq!(device.firmware1.unwrap(), 0x0007_0055);
        assert_eq!(device.firmware2.unwrap(), 0x11CB_0000);
        assert_eq!(device.characteristics.unwrap(), 0x8000_0000_0000_0020);
        assert_eq!(device.description.unwrap().as_deref(), Some("Firmware TPM"));
        for end in 0..0x1B {
            let mut short = record.clone();
            short.formatted.truncate(end);
            let device = tpm_devices(&table(short)).remove(0);
            assert_eq!(device.vendor.is_ok(), end >= 8);
            assert_eq!(device.spec.is_ok(), end >= 10);
            assert_eq!(device.firmware1.is_ok(), end >= 14);
            assert_eq!(device.firmware2.is_ok(), end >= 18);
            assert_eq!(device.description.is_ok(), end >= 19);
            assert!(device.characteristics.is_err());
        }
        for vendor in [*b"IF\0X", [0xFF; 4], *b"I\nX\0"] {
            record.formatted[4..8].copy_from_slice(&vendor);
            let device = tpm_devices(&table(record.clone())).remove(0);
            assert!(device.vendor.is_err());
            assert!(device.firmware1.is_ok());
        }
        record.formatted[4..8].fill(0);
        record.formatted[8..10].copy_from_slice(&[1, 2]);
        let device = tpm_devices(&table(record.clone())).remove(0);
        assert!(device.vendor.unwrap().is_none());
        assert_eq!(device.spec.unwrap(), (1, 2));
        record.formatted[8] = 0;
        assert!(tpm_devices(&table(record)).remove(0).spec.is_err());
    }

    #[test]
    fn type45_inventory_keeps_component_context_and_partial_fields() {
        let mut record = Structure {
            kind: 45,
            handle: 0x4501,
            formatted: vec![0; 0x18],
            strings: [
                "System Firmware",
                "2802",
                "e542bf75-2169-4e86-91c5-7c18a95d0634",
                "2023-09-27T00:00:00Z",
            ]
            .map(String::from)
            .to_vec(),
        };
        for (offset, index) in [(4, 1), (5, 2), (7, 3), (9, 4)] {
            record.formatted[offset] = index;
        }
        let table = |record: Structure| Smbios {
            major: 3,
            minor: 6,
            structures: vec![record],
        };
        let component = firmware_components(&table(record.clone())).remove(0);
        let mut repeated = table(record.clone());
        let mut second = record.clone();
        second.handle = 0x4502;
        repeated.structures.push(second);
        assert_eq!(
            firmware_components(&repeated)
                .iter()
                .map(|component| component.handle)
                .collect::<Vec<_>>(),
            [0x4501, 0x4502]
        );
        assert_eq!(component.handle, 0x4501);
        assert_eq!(component.name.unwrap().as_deref(), Some("System Firmware"));
        assert_eq!(component.version.unwrap().as_deref(), Some("2802"));
        assert_eq!(
            component.id.unwrap().as_deref(),
            Some("e542bf75-2169-4e86-91c5-7c18a95d0634")
        );
        assert_eq!(
            component.date.unwrap().as_deref(),
            Some("2023-09-27T00:00:00Z")
        );
        for end in 0..10 {
            let mut short = record.clone();
            short.formatted.truncate(end);
            let component = firmware_components(&table(short)).remove(0);
            for (offset, field) in [
                (4, component.name),
                (5, component.version),
                (7, component.id),
                (9, component.date),
            ] {
                assert_eq!(field.is_ok(), end > offset);
            }
        }
        record.formatted[4] = 0;
        record.formatted[7] = 255;
        let component = firmware_components(&table(record)).remove(0);
        assert!(component.name.unwrap().is_none());
        assert!(component.id.is_err());
        assert!(component.version.is_ok());
        assert!(component.date.is_ok());
    }

    #[test]
    fn type22_string_priority_sbds_encoding_and_bounds() {
        let mut record = Structure {
            kind: 22,
            handle: 0x2201,
            formatted: vec![0; 0x1A],
            strings: vec![
                "SMP".to_owned(),
                "2024-02-29".to_owned(),
                "BAT2402A7381".to_owned(),
                "L18M3P73".to_owned(),
            ],
        };
        record.formatted[5..9].copy_from_slice(&[1, 2, 3, 4]);
        record.formatted[0x10..0x12].copy_from_slice(&0x4A37_u16.to_le_bytes());
        record.formatted[0x12..0x14]
            .copy_from_slice(&(((2024 - 1980) << 9 | 2 << 5 | 29) as u16).to_le_bytes());
        let table = |record: Structure, minor| Smbios {
            major: 2,
            minor,
            structures: vec![record],
        };
        let decoded = portable_batteries(&table(record.clone(), 2));
        assert_eq!(
            decoded[0].serial.as_ref().unwrap().as_deref(),
            Some("BAT2402A7381")
        );
        assert_eq!(
            decoded[0].date.as_ref().unwrap().as_deref(),
            Some("2024-02-29")
        );
        assert_eq!(
            decoded[0].manufacturer.as_ref().unwrap().as_deref(),
            Some("SMP")
        );
        record.formatted[6..8].fill(0);
        let decoded = portable_batteries(&table(record.clone(), 2));
        assert_eq!(decoded[0].serial.as_ref().unwrap().as_deref(), Some("4A37"));
        assert_eq!(
            decoded[0].date.as_ref().unwrap().as_deref(),
            Some("2024-02-29")
        );
        let old = portable_batteries(&table(record.clone(), 1));
        assert!(old[0].serial.as_ref().unwrap().is_none());
        assert!(old[0].date.as_ref().unwrap().is_none());
        record.formatted[0x10..0x12].fill(0);
        assert_eq!(
            portable_batteries(&table(record.clone(), 2))[0]
                .serial
                .as_ref()
                .unwrap()
                .as_deref(),
            Some("0000")
        );
        for (year, month, day, valid) in [
            (2000, 2, 29, true),
            (2100, 2, 29, false),
            (1980, 1, 1, true),
            (2107, 12, 31, false),
            (2024, 0, 1, false),
            (2024, 13, 1, false),
            (2024, 4, 31, false),
            (2024, 1, 0, false),
        ] {
            let packed = ((year - 1980) << 9 | month << 5 | day) as u16;
            record.formatted[0x12..0x14].copy_from_slice(&packed.to_le_bytes());
            let decoded = portable_batteries(&table(record.clone(), 2));
            assert_eq!(decoded[0].date.is_ok(), valid);
            assert_eq!(
                decoded[0].manufacturer.as_ref().unwrap().as_deref(),
                Some("SMP")
            );
        }
        record.formatted[0x12..0x14].fill(0);
        assert!(
            portable_batteries(&table(record.clone(), 2))[0]
                .date
                .as_ref()
                .unwrap()
                .is_none()
        );
        record.formatted[7] = 99;
        let decoded = portable_batteries(&table(record.clone(), 2));
        assert!(decoded[0].serial.is_err());
        assert_eq!(
            decoded[0].name.as_ref().unwrap().as_deref(),
            Some("L18M3P73")
        );
        for end in 0..record.formatted.len() {
            let mut short = record.clone();
            short.formatted.truncate(end);
            assert_eq!(portable_batteries(&table(short, 2)).len(), 1);
        }
        record.kind = 21;
        assert!(portable_batteries(&table(record, 2)).is_empty());
    }

    // Fixtures are hex-encoded complete RawSMBIOSData buffers with fabricated values.
    fn fixture(hex: &str) -> Vec<u8> {
        hex.split_whitespace()
            .map(|byte| u8::from_str_radix(byte, 16).expect("valid fixture byte"))
            .collect()
    }

    #[test]
    fn zero_strings_consumes_both_nuls() -> Result<()> {
        let parsed = parse_smbios(&fixture(ZERO_STRINGS))?;
        assert_eq!((parsed.major, parsed.minor), (3, 2));
        assert_eq!(parsed.structures.len(), 2);
        assert_eq!(parsed.structures[0].kind, 2);
        assert_eq!(parsed.structures[0].handle, 0x0200);
        assert!(parsed.structures[0].strings.is_empty());
        assert_eq!(parsed.structures[1].kind, 1);
        assert_eq!(parsed.structures[1].string(1), "Desktop");
        Ok(())
    }

    #[test]
    fn truncated_last_structure_keeps_complete_prefix() -> Result<()> {
        let parsed = parse_smbios(&fixture(TRUNCATED))?;
        assert_eq!(parsed.structures.len(), 1);
        assert_eq!(parsed.structures[0].kind, 1);
        assert_eq!(parsed.structures[0].string(1), "XPS");
        Ok(())
    }

    #[test]
    fn repeated_types_keep_order_for_last_wins_consumers() -> Result<()> {
        let parsed = parse_smbios(&fixture(REPEATED))?;
        assert_eq!(parsed.structures.len(), 2);
        assert_eq!(parsed.structures[0].kind, 0);
        assert_eq!(parsed.structures[1].kind, 0);
        assert_eq!(parsed.structures[0].handle, 0x0100);
        assert_eq!(parsed.structures[1].handle, 0x0101);
        assert_eq!(parsed.structures[0].string(1), "1.20.0");
        assert_eq!(parsed.structures[1].string(1), "1.24.0");
        Ok(())
    }

    #[test]
    fn zero_and_out_of_range_string_indexes_are_empty() -> Result<()> {
        let parsed = parse_smbios(&fixture(INDEXES))?;
        let structure = &parsed.structures[0];
        assert_eq!(structure.string(0), "");
        assert_eq!(structure.string(1), "Dell");
        assert_eq!(structure.string(2), "Precision");
        assert_eq!(structure.string(9), "");
        assert_eq!(structure.string(u8::MAX), "");
        assert_eq!(structure.byte(4), Some(0));
        assert_eq!(structure.byte(7), Some(9));
        Ok(())
    }

    #[test]
    fn numeric_reads_check_width_and_overflow() {
        let structure = Structure {
            kind: 1,
            handle: 0x1234,
            formatted: vec![1, 16, 0x34, 0x12, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12],
            strings: Vec::new(),
        };
        assert_eq!(structure.word(2), Some(0x1234));
        assert_eq!(structure.dword(4), Some(0x0403_0201));
        assert_eq!(structure.qword(4), Some(0x0807_0605_0403_0201));
        assert_eq!(structure.qword(8), Some(0x0C0B_0A09_0807_0605));
        assert_eq!(structure.byte(16), None);
        assert_eq!(structure.word(15), None);
        assert_eq!(structure.dword(13), None);
        assert_eq!(structure.qword(9), None);
        for offset in [16, usize::MAX - 1, usize::MAX] {
            assert_eq!(structure.byte(offset), None);
            assert_eq!(structure.word(offset), None);
            assert_eq!(structure.dword(offset), None);
            assert_eq!(structure.qword(offset), None);
        }
    }

    #[test]
    fn ascii_replacement_and_unterminated_last_strings_match_csharp() -> Result<()> {
        let raw = [0, 3, 2, 0, 8, 0, 0, 0, 1, 5, 0, 1, 1, b'A', 0xE9, b'Z'];
        let parsed = parse_smbios(&raw)?;
        assert_eq!(parsed.structures.len(), 1);
        assert_eq!(parsed.structures[0].string(1), "A?Z");
        Ok(())
    }

    #[test]
    fn raw_header_bounds_and_end_marker_are_respected() -> Result<()> {
        assert!(parse_smbios(&[]).is_err());
        assert!(parse_smbios(&[0; 8]).is_err());
        assert!(parse_smbios(&[0, 3, 2, 0, 0xFF, 0xFF, 0xFF, 0xFF]).is_err());
        let mut raw = fixture(REPEATED);
        raw.extend_from_slice(&[1, 5, 0, 1, 1, b'X', 0, 0]);
        // Bytes beyond the declared table length are not parsed.
        assert_eq!(parse_smbios(&raw)?.structures.len(), 2);
        let length = (raw.len() - 8) as u32;
        raw[4..8].copy_from_slice(&length.to_le_bytes());
        // Nor are structures after type 127, even within the declared length.
        assert_eq!(parse_smbios(&raw)?.structures.len(), 2);
        Ok(())
    }

    #[test]
    fn truncated_and_mutated_buffers_never_panic() {
        for hex in [ZERO_STRINGS, TRUNCATED, REPEATED, INDEXES] {
            let raw = fixture(hex);
            for end in 0..=raw.len() {
                // Both a truncated outer buffer and a truncated advertised table must be safe.
                drop(parse_smbios(&raw[..end]));
                if end > 8 {
                    let mut shortened = raw[..end].to_vec();
                    shortened[4..8].copy_from_slice(&((end - 8) as u32).to_le_bytes());
                    drop(parse_smbios(&shortened));
                }
            }
            for index in 0..raw.len() {
                for byte in 0..=u8::MAX {
                    let mut mutated = raw.clone();
                    mutated[index] = byte;
                    // An Err is allowed; the assertion is that no byte or length can panic.
                    drop(parse_smbios(&mutated));
                }
            }
        }
    }

    #[test]
    fn rsmb_on_this_pc_contains_system_information() -> Result<()> {
        let parsed = smbios()?;
        assert!(
            parsed
                .structures
                .iter()
                .any(|structure| structure.kind == 1)
        );
        Ok(())
    }
}
