//! Bounded JEDEC manufacturing-field decoder; no hardware access.
//!
//! DDR4: JESD21-C Annex L (SPD revision 1.x); DDR5: JESD400-5 (1.x).
//! Byte order is explicit: SPD bytes are not host-endian words.

use super::{Error, Result};

pub const MAX_BYTES: usize = 1024;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Generation {
    Ddr4,
    Ddr5,
}

#[derive(Clone, Debug)]
pub struct Manufacturing {
    pub generation: Generation,
    pub manufacturer: Option<String>,
    pub part_number: Option<String>,
    pub serial: Option<String>,
    pub failures: Vec<Error>,
}

/// Restores adjacent bytes in a capture explicitly supplied as swapped 16-bit words.
/// Never guesses byte order from printable part numbers or plausible serials.
pub fn normalize_words(bytes: &mut [u8]) -> Result<()> {
    if bytes.len() > MAX_BYTES || !bytes.len().is_multiple_of(2) {
        return Err(malformed("invalid word capture length"));
    }
    for word in bytes.as_chunks_mut::<2>().0 {
        word.swap(0, 1);
    }
    Ok(())
}

pub fn decode(bytes: &[u8]) -> Result<Manufacturing> {
    if bytes.len() < 4 || bytes.len() > MAX_BYTES {
        return Err(malformed("invalid SPD length"));
    }
    if bytes[1] >> 4 != 1 {
        return Err(malformed("unsupported SPD layout revision"));
    }
    let (generation, maker, serial, part, part_len) = match bytes[2] {
        0x0c => {
            let used = match bytes[0] & 0x0f {
                3 => 384,
                4 => 512,
                _ => return Err(malformed("DDR4 manufacturing region not declared")),
            };
            if bytes[0] & 0xf0 != 0x20 || bytes.len() != 512 || used > bytes.len() {
                return Err(malformed("invalid or truncated DDR4 declaration"));
            }
            check_crc(bytes, 0, 126)?;
            check_crc(bytes, 128, 254)?;
            (Generation::Ddr4, 320, 325, 329, 20)
        }
        0x12 => {
            if bytes[0] != 0x30 || bytes.len() != 1024 {
                return Err(malformed("invalid or truncated DDR5 declaration"));
            }
            check_crc(bytes, 0, 510)?;
            (Generation::Ddr5, 512, 517, 521, 30)
        }
        _ => return Err(malformed("unsupported memory type")),
    };
    let mut result = Manufacturing {
        generation,
        manufacturer: None,
        part_number: None,
        serial: None,
        failures: Vec::new(),
    };
    let bank = bytes[maker] & 0x7f;
    let code = bytes[maker + 1];
    if code == 0 || code == 0xff || code.count_ones().is_multiple_of(2) {
        result
            .failures
            .push(malformed("manufacturer ID absent or invalid JEP106 parity"));
    } else {
        let name = match (bank, code) {
            (0, 0x2c) => "Micron",
            (0, 0xce) => "Samsung",
            (0, 0xad) => "SK hynix",
            (1, 0x98) => "Kingston",
            (2, 0x9e) => "Corsair",
            (4, 0xcb) => "A-DATA",
            _ => "JEP106",
        };
        result.manufacturer = Some(format!("{name} (bank {}, code 0x{code:02X})", bank + 1));
    }
    // An all-00/FF serial is what the module vendor programmed (SMBIOS shows it too); report it as read.
    result.serial = Some(
        bytes[serial..serial + 4]
            .iter()
            .map(|byte| format!("{byte:02X}"))
            .collect(),
    );
    // JEDEC pads with spaces, but some vendors pad with NUL bytes.
    let text = &bytes[part..part + part_len];
    let text = &text[..text
        .iter()
        .rposition(|byte| *byte != 0)
        .map_or(0, |end| end + 1)];
    if text.iter().all(|byte| (0x20..=0x7e).contains(byte)) {
        let text = String::from_utf8_lossy(text).trim().to_owned();
        if !text.is_empty() {
            result.part_number = Some(text);
        } else {
            result.failures.push(malformed("part number absent"));
        }
    } else {
        result
            .failures
            .push(malformed("part number is not printable ASCII"));
    }
    Ok(result)
}

fn malformed(detail: &'static str) -> Error {
    Error::msg("SPD decode", detail)
}

fn check_crc(bytes: &[u8], start: usize, end: usize) -> Result<()> {
    if crc16(&bytes[start..end]) != u16::from_le_bytes([bytes[end], bytes[end + 1]]) {
        return Err(malformed("required SPD CRC mismatch"));
    }
    Ok(())
}

// JEDEC CRC-16/CCITT: polynomial 0x1021, initial value zero, no reflection/xor.
fn crc16(bytes: &[u8]) -> u16 {
    let mut crc = 0_u16;
    for byte in bytes {
        crc ^= u16::from(*byte) << 8;
        for _ in 0..8 {
            crc = (crc << 1) ^ if crc & 0x8000 != 0 { 0x1021 } else { 0 };
        }
    }
    crc
}

#[cfg(test)]
pub(crate) fn fixture(generation: Generation) -> Vec<u8> {
    let (length, maker, serial, part, part_len, name) = match generation {
        Generation::Ddr4 => (512, 320, 325, 329, 20, "KF432C16BB1/16"),
        Generation::Ddr5 => (1024, 512, 517, 521, 30, "KF556C40BB-16"),
    };
    let mut bytes = vec![0; length];
    bytes[0] = if length == 512 { 0x23 } else { 0x30 };
    bytes[1] = 0x11;
    bytes[2] = if length == 512 { 0x0c } else { 0x12 };
    bytes[3] = 2;
    bytes[maker..maker + 2].copy_from_slice(&[0x01, 0x98]);
    bytes[serial..serial + 4].copy_from_slice(&[0x8c, 0x31, 0xd7, 0xa2]);
    bytes[part..part + part_len].fill(b' ');
    bytes[part..part + name.len()].copy_from_slice(name.as_bytes());
    for (start, end) in if length == 512 {
        vec![(0, 126), (128, 254)]
    } else {
        vec![(0, 510)]
    } {
        let crc = crc16(&bytes[start..end]);
        bytes[end..end + 2].copy_from_slice(&crc.to_le_bytes());
    }
    bytes
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fabricated_generations_decode_and_word_captures_restore_exact_bytes() {
        assert_eq!(crc16(b"123456789"), 0x31c3);
        for generation in [Generation::Ddr4, Generation::Ddr5] {
            let original = fixture(generation);
            let result = decode(&original).unwrap();
            assert_eq!(result.generation, generation);
            assert_eq!(result.serial.as_deref(), Some("8C31D7A2"));
            assert!(result.manufacturer.unwrap().starts_with("Kingston"));
            assert!(result.part_number.unwrap().starts_with("KF"));
            assert!(result.failures.is_empty());
            let mut swapped = original.clone();
            for word in swapped.as_chunks_mut::<2>().0 {
                word.swap(0, 1);
            }
            assert!(decode(&swapped).is_err());
            normalize_words(&mut swapped).unwrap();
            assert_eq!(swapped, original);
            assert_eq!(decode(&swapped).unwrap().serial, result.serial);
            assert!(decode(&original[..original.len() - 1]).is_err());
            let mut bad = original.clone();
            bad[20] ^= 1;
            assert!(decode(&bad).unwrap_err().detail.contains("CRC"));
            if generation == Generation::Ddr4 {
                let mut bad = original.clone();
                bad[200] ^= 1;
                assert!(decode(&bad).is_err());
            }
            for offset in [0, 1, 2] {
                let mut bad = original.clone();
                bad[offset] = 0xff;
                assert!(decode(&bad).is_err());
            }
        }
        assert!(normalize_words(&mut [0; 3]).is_err());
        assert!(decode(&[]).is_err());
        assert!(decode(&vec![0; MAX_BYTES + 1]).is_err());
    }

    #[test]
    fn malformed_optional_fields_preserve_independent_values_without_payload_in_errors() {
        for (generation, serial, part, maker) in [
            (Generation::Ddr4, 325, 329, 320),
            (Generation::Ddr5, 517, 521, 512),
        ] {
            for (sentinel, shown) in [(0, "00000000"), (0xff, "FFFFFFFF")] {
                let mut bytes = fixture(generation);
                bytes[serial..serial + 4].fill(sentinel);
                // NUL padding instead of spaces, as some vendors program it.
                let part_len = if generation == Generation::Ddr4 {
                    20
                } else {
                    30
                };
                for byte in &mut bytes[part..part + part_len] {
                    if *byte == b' ' {
                        *byte = 0;
                    }
                }
                let result = decode(&bytes).unwrap();
                assert_eq!(result.serial.as_deref(), Some(shown));
                assert!(result.part_number.unwrap().starts_with("KF"));
                assert!(result.failures.is_empty());
            }
            let mut bytes = fixture(generation);
            bytes[part] = b'\n';
            bytes[maker + 1] = 0xff;
            let result = decode(&bytes).unwrap();
            assert!(result.manufacturer.is_none() && result.part_number.is_none());
            assert!(result.serial.is_some());
            assert!(
                result
                    .failures
                    .iter()
                    .all(|e| !e.detail.contains("8C31D7A2"))
            );
        }
    }
}
