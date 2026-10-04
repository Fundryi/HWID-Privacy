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
    if value.chars().any(char::is_control) {
        return Err(Error::msg("SMBIOS battery", "control character in string"));
    }
    Ok((!value.is_empty()).then(|| value.clone()))
}

/// Checks the calendar date used by battery IOCTL and SBDS encodings.
pub fn battery_date(year: u16, month: u8, day: u8) -> Result<String> {
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
    Ok(format!("{year:04}-{month:02}-{day:02}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    const ZERO_STRINGS: &str = include_str!("../../tests/fixtures/phase1-smbios/zero-strings.hex");
    const TRUNCATED: &str = include_str!("../../tests/fixtures/phase1-smbios/truncated-last.hex");
    const REPEATED: &str = include_str!("../../tests/fixtures/phase1-smbios/repeated-types.hex");
    const INDEXES: &str = include_str!("../../tests/fixtures/phase1-smbios/string-indexes.hex");

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
            (2107, 12, 31, true),
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
