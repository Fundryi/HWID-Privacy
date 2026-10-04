//! Owned by WP-08: monitor EDID decoding.

use super::{Error, Result};

/// Decoded base-block identity fields, retaining descriptor order and checksum evidence.
#[derive(Debug)]
pub struct Edid {
    /// Legacy three-letter PNP manufacturer decode.
    pub manufacturer: String,
    /// Model and string-serial descriptors in base-block order.
    pub descriptors: Vec<(&'static str, String)>,
    /// Little-endian serial after suppressing the three C# numeric placeholders.
    pub numeric_serial: Option<u32>,
    /// Little-endian manufacturer-assigned product code (including zero).
    pub product_code: u16,
    /// Base-block date; reserved week/version encodings are not dates.
    pub date: Option<Date>,
    /// Whether the complete base-block byte sum is zero modulo 256.
    pub checksum_valid: bool,
}

/// EDID distinguishes an unspecified manufacture week from a model year.
#[derive(Debug, PartialEq, Eq)]
pub enum Date {
    /// Manufacture year with an optional week (zero means unspecified).
    Manufactured { week: Option<u8>, year: u16 },
    /// EDID 1.4 week 0xff denotes a model year, not a manufacture date.
    ModelYear(u16),
}

/// Maps a WmiMonitorID instance name to its exact DISPLAY device instance ID.
pub fn instance_id(name: &str) -> Result<String> {
    let name = name.strip_suffix("_0").unwrap_or(name);
    let parts: Vec<_> = name.split('\\').collect();
    if parts.len() != 3
        || !parts[0].eq_ignore_ascii_case("DISPLAY")
        || parts.iter().any(|part| {
            part.is_empty() || *part == "." || *part == ".." || part.contains(['\0', '/'])
        })
    {
        return Err(Error::msg(
            "WmiMonitorID.InstanceName",
            "expected DISPLAY\\monitor\\instance",
        ));
    }
    Ok(name.to_ascii_uppercase())
}

/// Parses a complete EDID base block without discarding data on checksum failure.
pub fn parse(bytes: &[u8]) -> Result<Edid> {
    if bytes.len() < 128 {
        return Err(Error::msg("EDID", "base block is shorter than 128 bytes"));
    }
    if bytes[..8] != [0, 255, 255, 255, 255, 255, 255, 0] {
        return Err(Error::msg("EDID", "invalid base-block header"));
    }
    // C# parity: Hardware/MonitorInfo.cs:161-167. Keep the legacy 5-bit letter decode.
    let manufacturer_code = u16::from_be_bytes([bytes[8], bytes[9]]);
    let manufacturer = [10, 5, 0]
        .into_iter()
        .map(|shift| char::from(((manufacturer_code >> shift) & 31) as u8 + b'A' - 1))
        .collect();
    let mut descriptors = Vec::new();
    // C# parity: Hardware/MonitorInfo.cs:204-217. Only the first two bytes gate a
    // descriptor; preserve duplicate tags, order, leading spaces and ASCII replacement.
    for offset in (54..=108).step_by(18) {
        if bytes[offset] != 0 || bytes[offset + 1] != 0 {
            continue;
        }
        let label = match bytes[offset + 3] {
            0xfc => "Model",
            0xff => "Serial Number",
            _ => continue,
        };
        let text: String = bytes[offset + 5..offset + 18]
            .iter()
            .map(|&byte| char::from(if byte < 128 { byte } else { b'?' }))
            .collect();
        let text = text.trim_end_matches(['\n', '\r', ' ', '\0']);
        if !text.is_empty() {
            descriptors.push((label, text.to_owned()));
        }
    }
    let serial = u32::from_le_bytes([bytes[12], bytes[13], bytes[14], bytes[15]]);
    // C# parity: Hardware/MonitorInfo.cs:239-243. Only these three numeric placeholders hide.
    let numeric_serial = (!matches!(serial, 0 | 0xffff_ffff | 0x0101_0101)).then_some(serial);
    let year = 1990 + u16::from(bytes[17]);
    // EDID 1.0-1.3 allow weeks 1-53; 1.4 adds week 54 and model year (0xff).
    // Unknown versions and reserved weeks must not invent a manufacture date.
    let date = match (bytes[18], bytes[19], bytes[16]) {
        (1, 0..=4, 0) => Some(Date::Manufactured { week: None, year }),
        (1, 0..=4, week @ 1..=53) | (1, 4, week @ 54) => Some(Date::Manufactured {
            week: Some(week),
            year,
        }),
        (1, 4, 255) => Some(Date::ModelYear(year)),
        _ => None,
    };
    Ok(Edid {
        manufacturer,
        descriptors,
        numeric_serial,
        product_code: u16::from_le_bytes([bytes[10], bytes[11]]),
        date,
        checksum_valid: bytes[..128]
            .iter()
            .fold(0_u8, |sum, byte| sum.wrapping_add(*byte))
            == 0,
    })
}

/// Identity fields from one validated driver-exposed extension, in descriptor order.
#[derive(Debug, Default)]
pub struct Extension {
    pub source: &'static str,
    pub fields: Vec<(&'static str, String, bool)>,
    pub failures: Vec<Error>,
}

/// Validates exactly one E-EDID block, including its extension checksum.
pub fn validate_block(bytes: &[u8]) -> Result<()> {
    if bytes.len() != 128 {
        return Err(Error::msg("EDID extension", "expected exactly 128 bytes"));
    }
    if byte_sum(bytes) != 0 {
        return Err(Error::msg("EDID extension", "invalid block checksum"));
    }
    Ok(())
}

fn byte_sum(bytes: &[u8]) -> u8 {
    bytes.iter().fold(0_u8, |sum, byte| sum.wrapping_add(*byte))
}

/// Parses only specified identity layouts; timing and opaque vendor payloads are ignored.
pub fn parse_extension(bytes: &[u8]) -> Result<Extension> {
    validate_block(bytes)?;
    let [tag, version, length, ..] = bytes else {
        return Err(Error::msg("EDID extension", "malformed: truncated header"));
    };
    let mut extension = Extension::default();
    match *tag {
        0x70 => {
            extension.source = "DisplayID";
            // Structure versions 1.0-1.3 and 2.0. The standard document's version
            // 2.1a still uses structure version 2.0 (VESA v2.1a section 2.1).
            if !matches!(*version, 0x10..=0x13 | 0x20) {
                return Err(unsupported("unsupported DisplayID structure version"));
            }
            let end = 5 + usize::from(*length);
            if end > 126 {
                return Err(Error::msg(
                    "DisplayID",
                    "payload exceeds extension boundary",
                ));
            }
            // The DisplayID checksum excludes the EDID tag and includes its own
            // checksum byte immediately after the payload (independent of byte 127).
            let section = bytes
                .get(1..=end)
                .ok_or_else(|| Error::msg("DisplayID", "malformed: section bounds"))?;
            if byte_sum(section) != 0 {
                return Err(Error::msg("DisplayID", "invalid section checksum"));
            }
            let mut remaining = bytes
                .get(5..end)
                .ok_or_else(|| Error::msg("DisplayID", "malformed: payload bounds"))?;
            for _ in 0..=MAX_DATA_BLOCKS {
                if remaining.iter().all(|byte| *byte == 0) {
                    break; // Specified zero filler, not an empty product descriptor.
                }
                let [tag, revision, length, tail @ ..] = remaining else {
                    extension
                        .failures
                        .push(Error::msg("DisplayID", "truncated data-block header"));
                    break;
                };
                let Some(payload) = tail.get(..usize::from(*length)) else {
                    extension.failures.push(Error::msg(
                        "DisplayID",
                        "data block exceeds section payload",
                    ));
                    break;
                };
                let product =
                    (*version < 0x20 && *tag == 0x00) || (*version == 0x20 && *tag == 0x20);
                if product {
                    if *revision == 0 {
                        displayid_product(&mut extension, payload, *version == 0x20);
                    } else {
                        extension
                            .failures
                            .push(unsupported("unsupported product-block revision"));
                    }
                } else if *version < 0x20 && *tag == 0x0a {
                    // DisplayID 1.x Product Serial Number ASCII data block.
                    if *revision == 0 {
                        extension_text(&mut extension, "Serial", payload, true);
                    } else {
                        extension
                            .failures
                            .push(unsupported("unsupported serial-block revision"));
                    }
                } else if matches!(*tag, 0x00 | 0x20 | 0x0a) {
                    extension
                        .failures
                        .push(unsupported("identity tag does not match DisplayID version"));
                }
                remaining = tail
                    .get(usize::from(*length)..)
                    .ok_or_else(|| Error::msg("DisplayID", "malformed: next block bounds"))?;
            }
        }
        0x02 => {
            extension.source = "CTA-861";
            // Revision 3 introduced the data-block collection used by PIDB.
            if *version != 3 {
                return Err(unsupported("unsupported CTA extension revision"));
            }
            let end = usize::from(*length);
            if end == 0 {
                return Ok(extension); // No data-block collection or detailed timings.
            }
            if !(4..=127).contains(&end) {
                return Err(Error::msg(
                    "CTA-861",
                    "invalid data-block collection boundary",
                ));
            }
            let mut remaining = bytes
                .get(4..end)
                .ok_or_else(|| Error::msg("CTA-861", "malformed: collection bounds"))?;
            for _ in 0..=MAX_DATA_BLOCKS {
                let Some((&header, tail)) = remaining.split_first() else {
                    break;
                };
                let length = usize::from(header & 31);
                let Some(payload) = tail.get(..length) else {
                    extension.failures.push(Error::msg(
                        "CTA-861",
                        "data block exceeds collection boundary",
                    ));
                    break;
                };
                if header >> 5 == 7 && payload.first() == Some(&0x21) {
                    // CTA-861.7 section 7.5.20, table 122: ext-tag, OUI/CID LSB
                    // first, optional version, then at most 25 ASCII model bytes.
                    if let Some([_, a, b, c]) = payload.get(..4) {
                        append_oui(&mut extension, &[*c, *b, *a]);
                        if let Some(version) = payload.get(4) {
                            if *version == 0 {
                                if let Some(name) = payload.get(5..) {
                                    extension_text(&mut extension, "Model", name, false);
                                }
                            } else {
                                extension
                                    .failures
                                    .push(unsupported("unsupported CTA product-block version"));
                            }
                        }
                    } else {
                        extension
                            .failures
                            .push(Error::msg("CTA-861 PIDB", "truncated manufacturer field"));
                    }
                }
                remaining = tail
                    .get(length..)
                    .ok_or_else(|| Error::msg("CTA-861", "malformed: next block bounds"))?;
            }
        }
        _ => {
            return Err(unsupported(
                "extension tag has no supported identity layout",
            ));
        }
    }
    Ok(extension)
}

// A 128-byte EDID extension cannot contain more than 127 one-byte blocks.
const MAX_DATA_BLOCKS: usize = 127;

fn unsupported(detail: &'static str) -> Error {
    Error {
        op: "EDID extension",
        code: 50,
        detail: detail.into(),
    }
}

fn displayid_product(extension: &mut Extension, payload: &[u8], oui: bool) {
    // VESA DisplayID v2.1a table 4-1; v1.x tag 0x00 has the same offsets,
    // but its three-byte manufacturer field is ASCII PNP, not an IEEE OUI.
    let Some([a, b, c, lo, hi, s0, s1, s2, s3, _, _, size]) = payload.get(..12) else {
        extension.failures.push(Error::msg(
            "DisplayID product",
            "payload is shorter than 12 bytes",
        ));
        return;
    };
    let vendor = [*a, *b, *c];
    if oui {
        append_oui(extension, &vendor);
    } else if vendor.iter().all(u8::is_ascii_uppercase) {
        extension.fields.push((
            "Manufacturer",
            String::from_utf8_lossy(&vendor).into(),
            false,
        ));
    } else {
        extension
            .failures
            .push(Error::msg("DisplayID product", "invalid PNP manufacturer"));
    }
    extension.fields.push((
        "Product Code",
        format!("{:04X}", u16::from_le_bytes([*lo, *hi])),
        false,
    ));
    let serial = u32::from_le_bytes([*s0, *s1, *s2, *s3]);
    if !matches!(serial, 0 | u32::MAX) {
        extension.fields.push(("Serial", serial.to_string(), true));
    } else {
        extension.failures.push(Error::msg(
            "DisplayID product serial",
            "implausible: zero or all-ones serial omitted",
        ));
    }
    let size = usize::from(*size);
    if 12 + size != payload.len() {
        extension.failures.push(Error::msg(
            "DisplayID product",
            "product-name length does not match payload",
        ));
    } else {
        if let Some(name) = payload.get(12..) {
            extension_text(extension, "Model", name, false);
        }
    }
}

fn append_oui(extension: &mut Extension, bytes: &[u8]) {
    let [a, b, c] = bytes else {
        extension
            .failures
            .push(Error::msg("EDID manufacturer OUI", "malformed OUI length"));
        return;
    };
    if bytes != [0, 0, 0] && bytes != [255, 255, 255] {
        extension.fields.push((
            "Manufacturer OUI",
            format!("{a:02X}-{b:02X}-{c:02X}"),
            false,
        ));
    } else {
        extension.failures.push(Error::msg(
            "EDID manufacturer OUI",
            "placeholder: zero or all-ones OUI omitted",
        ));
    }
}

fn extension_text(extension: &mut Extension, label: &'static str, bytes: &[u8], id: bool) {
    let bytes = bytes.trim_ascii_end();
    let end = bytes
        .iter()
        .rposition(|byte| *byte != 0)
        .map_or(0, |i| i + 1);
    let Some(bytes) = bytes.get(..end) else {
        extension
            .failures
            .push(Error::msg("EDID extension text", "malformed: text bounds"));
        return;
    };
    if !bytes.iter().all(|byte| (0x20..=0x7e).contains(byte)) {
        extension.failures.push(Error::msg(
            "EDID extension text",
            "non-printable or non-ASCII text",
        ));
    } else if !bytes.is_empty() {
        if id
            && (bytes.iter().all(|byte| Some(byte) == bytes.first())
                || matches!(
                    bytes.to_ascii_lowercase().as_slice(),
                    b"unknown" | b"none" | b"n/a"
                ))
        {
            extension.failures.push(Error::msg(
                "EDID extension serial",
                "placeholder: serial omitted",
            ));
            return;
        }
        extension
            .fields
            .push((label, String::from_utf8_lossy(bytes).into(), id));
    } else {
        extension.failures.push(Error::msg(
            "EDID extension text",
            "absent: empty optional text",
        ));
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)] // Fixture assertions may panic; production code may not.
mod tests {
    use super::*;

    #[test]
    fn extension_placeholders_leave_product_context_and_diagnostics() {
        for serial in [0, u32::MAX] {
            let parsed = parse_extension(&extension_fixture(
                0x20,
                &product_fixture(0x20, &[0, 0x10, 0xfa], serial),
            ))
            .unwrap();
            assert!(!parsed.fields.iter().any(|(_, _, identity)| *identity));
            assert!(
                parsed
                    .fields
                    .iter()
                    .any(|(label, _, _)| *label == "Product Code")
            );
            assert!(
                parsed
                    .failures
                    .iter()
                    .any(|failure| failure.detail.contains("implausible"))
            );
        }
        let parsed = parse_extension(&extension_fixture(0x13, b"\x0a\0\x04FFFF")).unwrap();
        assert!(parsed.fields.is_empty());
        assert_eq!(parsed.failures.len(), 1);
        let mut extension = Extension::default();
        append_oui(&mut extension, &[0; 2]);
        append_oui(&mut extension, &[255; 3]);
        assert!(extension.fields.is_empty());
        assert_eq!(extension.failures.len(), 2);
    }

    fn extension_fixture(version: u8, blocks: &[u8]) -> [u8; 128] {
        let mut bytes = [0; 128];
        bytes[0] = 0x70;
        bytes[1] = version;
        bytes[2] = blocks.len() as u8;
        bytes[3] = 3;
        bytes[5..5 + blocks.len()].copy_from_slice(blocks);
        bytes[5 + blocks.len()] = 0_u8.wrapping_sub(byte_sum(&bytes[1..5 + blocks.len()]));
        bytes[127] = 0_u8.wrapping_sub(byte_sum(&bytes[..127]));
        bytes
    }

    fn product_fixture(tag: u8, vendor: &[u8; 3], serial: u32) -> Vec<u8> {
        let mut block = vec![tag, 0, 19];
        block.extend_from_slice(vendor);
        block.extend_from_slice(&0xa1f4_u16.to_le_bytes());
        block.extend_from_slice(&serial.to_le_bytes());
        block.extend_from_slice(&[18, 24, 7]);
        block.extend_from_slice(b"U2723QE");
        block
    }

    #[test]
    fn extensions_decode_versioned_products_and_serial_text_in_order() {
        for (version, tag, vendor, manufacturer) in [
            (0x13, 0x00, *b"DEL", "DEL"),
            (0x20, 0x20, [0x00, 0x10, 0xfa], "00-10-FA"),
        ] {
            let mut blocks = product_fixture(tag, &vendor, 1937468251);
            if version < 0x20 {
                blocks.extend_from_slice(b"\x0a\0\x07");
                blocks.extend_from_slice(b"8VJ6M47");
            }
            let parsed = parse_extension(&extension_fixture(version, &blocks)).unwrap();
            assert!(parsed.failures.is_empty());
            assert_eq!(parsed.fields[0].1, manufacturer);
            assert_eq!(parsed.fields[1], ("Product Code", "A1F4".into(), false));
            assert_eq!(parsed.fields[2], ("Serial", "1937468251".into(), true));
            assert_eq!(parsed.fields[3], ("Model", "U2723QE".into(), false));
            if version < 0x20 {
                assert_eq!(parsed.fields[4], ("Serial", "8VJ6M47".into(), true));
            }
        }
        let parsed = parse_extension(&extension_fixture(
            0x20,
            &product_fixture(0x20, &[0, 0x10, 0xfa], 0),
        ))
        .unwrap();
        assert!(!parsed.fields.iter().any(|(label, _, _)| *label == "Serial"));
    }

    #[test]
    fn extensions_validate_both_checksums_and_outer_lengths() {
        let valid = extension_fixture(0x20, &product_fixture(0x20, &[0, 0x10, 0xfa], 1937468251));
        for length in 0..128 {
            assert!(parse_extension(&valid[..length]).is_err());
        }
        let mut long = valid.to_vec();
        long.push(0);
        assert!(parse_extension(&long).is_err());
        let mut bad = valid;
        bad[127] ^= 1;
        assert!(parse_extension(&bad).is_err());
        bad = valid;
        bad[24] ^= 1;
        bad[127] = 0_u8.wrapping_sub(byte_sum(&bad[..127]));
        assert!(parse_extension(&bad).is_err()); // Outer checksum cannot hide a bad DisplayID checksum.
        bad = valid;
        bad[2] = 122;
        bad[127] = 0_u8.wrapping_sub(byte_sum(&bad[..127]));
        assert!(parse_extension(&bad).is_err());
        assert!(parse_extension(&extension_fixture(0x21, &[])).is_err());
    }

    #[test]
    fn extensions_keep_independent_fields_on_malformed_descriptors() {
        let mut block = product_fixture(0x20, &[0, 0x10, 0xfa], 1937468251);
        block[14] = 8; // Claimed name size exceeds this descriptor; fixed fields survive.
        let parsed = parse_extension(&extension_fixture(0x20, &block)).unwrap();
        assert_eq!(parsed.fields.len(), 3);
        assert_eq!(parsed.failures.len(), 1);
        block = product_fixture(0x20, &[0, 0x10, 0xfa], 1937468251);
        block.extend_from_slice(&[0x20, 0, 120]); // Later framing failure retains earlier product.
        let parsed = parse_extension(&extension_fixture(0x20, &block)).unwrap();
        assert_eq!(parsed.fields.len(), 4);
        assert_eq!(parsed.failures.len(), 1);
        let mismatched = parse_extension(&extension_fixture(
            0x20,
            &product_fixture(0, b"DEL", 1937468251),
        ))
        .unwrap();
        assert!(mismatched.fields.is_empty());
        assert_eq!(mismatched.failures.len(), 1);
        block = product_fixture(0x20, &[0, 0x10, 0xfa], 1937468251);
        block[1] = 1;
        assert!(
            parse_extension(&extension_fixture(0x20, &block))
                .unwrap()
                .fields
                .is_empty()
        );
        let serial = parse_extension(&extension_fixture(0x13, b"\x0a\0\x04AB\nC")).unwrap();
        assert!(serial.fields.is_empty());
        assert_eq!(serial.failures.len(), 1);
    }

    #[test]
    fn cta_product_information_uses_little_endian_oui_and_bounded_ascii() {
        let mut bytes = [0; 128];
        bytes[0] = 2;
        bytes[1] = 3;
        let data = [0xe9, 0x21, 0xfa, 0x10, 0, 0, b'D', b'E', b'L', b'L'];
        bytes[2] = (4 + data.len()) as u8;
        bytes[4..4 + data.len()].copy_from_slice(&data);
        bytes[127] = 0_u8.wrapping_sub(byte_sum(&bytes[..127]));
        let parsed = parse_extension(&bytes).unwrap();
        assert_eq!(
            parsed.fields,
            [
                ("Manufacturer OUI", "00-10-FA".into(), false),
                ("Model", "DELL".into(), false)
            ]
        );
        assert!(parsed.failures.is_empty());
        bytes[9] = 1;
        bytes[127] = 0_u8.wrapping_sub(byte_sum(&bytes[..127]));
        let parsed = parse_extension(&bytes).unwrap();
        assert_eq!(parsed.fields.len(), 1);
        assert_eq!(parsed.failures.len(), 1);
        bytes[4] = 0xff;
        bytes[127] = 0_u8.wrapping_sub(byte_sum(&bytes[..127]));
        let parsed = parse_extension(&bytes).unwrap();
        assert!(parsed.fields.is_empty());
        assert_eq!(parsed.failures.len(), 1);
        bytes[2] = 3;
        bytes[127] = 0_u8.wrapping_sub(byte_sum(&bytes[..127]));
        assert!(parse_extension(&bytes).is_err());
    }

    fn fixture() -> serde_json::Value {
        serde_json::from_str(include_str!("../../tests/fixtures/wp-08/monitors.json")).unwrap()
    }

    fn bytes(record: &serde_json::Value) -> Vec<u8> {
        let hex = record["edid_hex"].as_str().unwrap();
        (0..hex.len())
            .step_by(2)
            .map(|offset| u8::from_str_radix(&hex[offset..offset + 2], 16).unwrap())
            .collect()
    }

    #[test]
    fn wp08_exact_instances_keep_same_brand_and_historical_serials_separate() {
        let fixture = fixture();
        let records = fixture["registry"].as_array().unwrap();
        for mapping in fixture["wmi"].as_array().unwrap() {
            let name = mapping["instance_name"]
                .as_str()
                .unwrap()
                .to_ascii_lowercase();
            let id = instance_id(&name).unwrap();
            let matched = records.iter().find(|record| {
                record["instance_id"]
                    .as_str()
                    .unwrap()
                    .eq_ignore_ascii_case(&id)
            });
            let serial = matched.and_then(|record| parse(&bytes(record)).unwrap().numeric_serial);
            assert_eq!(serial.map(u64::from), mapping["expected_serial"].as_u64());
        }
        for record in records {
            let parsed = parse(&bytes(record)).unwrap();
            assert!(parsed.checksum_valid);
            assert_eq!(parsed.manufacturer, "DEL");
            assert_eq!(parsed.product_code, 0x4321);
            assert_eq!(
                parsed.date,
                Some(Date::Manufactured {
                    week: Some(record["week"].as_u64().unwrap() as u8),
                    year: record["year"].as_u64().unwrap() as u16,
                })
            );
            assert_eq!(
                parsed.numeric_serial.map(u64::from),
                record["numeric_serial"].as_u64()
            );
            assert_eq!(
                parsed.descriptors,
                [
                    ("Model", record["model"].as_str().unwrap().to_owned()),
                    (
                        "Serial Number",
                        record["string_serial"].as_str().unwrap().to_owned()
                    )
                ]
            );
        }
        for bad in [
            "",
            "PCI\\DEL4321\\x_0",
            "DISPLAY\\DEL4321",
            "DISPLAY\\..\\x_0",
            "DISPLAY\\DEL4321\\x\0_0",
        ] {
            assert!(instance_id(bad).is_err(), "{bad:?}");
        }
        assert_eq!(
            instance_id(r"DISPLAY\DEL4321\INSTANCE_0_0").unwrap(),
            r"DISPLAY\DEL4321\INSTANCE_0"
        );
    }

    #[test]
    fn wp08_parser_rejects_short_headers_and_retains_bad_checksum_data() {
        let fixture = fixture();
        let mut base = bytes(&fixture["registry"][0]);
        for length in fixture["truncated_lengths"].as_array().unwrap() {
            assert!(parse(&base[..length.as_u64().unwrap() as usize]).is_err());
        }
        base[0] = 1;
        assert!(parse(&base).is_err());
        base[0] = 0;
        base[127] ^= 1;
        let parsed = parse(&base).unwrap();
        assert!(!parsed.checksum_valid);
        assert_eq!(parsed.numeric_serial, Some(1248566305));
        for serial in fixture["placeholder_serials"].as_array().unwrap() {
            base[12..16].copy_from_slice(&(serial.as_u64().unwrap() as u32).to_le_bytes());
            assert!(parse(&base).unwrap().numeric_serial.is_none());
        }
        base[10..12].copy_from_slice(&[0x34, 0x12]);
        assert_eq!(parse(&base).unwrap().product_code, 0x1234);
        base[10..12].fill(0);
        assert_eq!(parse(&base).unwrap().product_code, 0);
        for case in fixture["date_cases"].as_array().unwrap() {
            let raw = case["bytes_16_to_19"].as_array().unwrap();
            for (target, value) in base[16..20].iter_mut().zip(raw) {
                *target = value.as_u64().unwrap() as u8;
            }
            let expected = match case["kind"].as_str().unwrap() {
                "manufactured" => Some(Date::Manufactured {
                    week: case["week"].as_u64().map(|week| week as u8),
                    year: case["year"].as_u64().unwrap() as u16,
                }),
                "model" => Some(Date::ModelYear(case["year"].as_u64().unwrap() as u16)),
                _ => None,
            };
            assert_eq!(parse(&base).unwrap().date, expected, "{case}");
        }
    }

    #[test]
    fn wp08_descriptor_order_duplicates_and_ascii_trim_match_csharp() {
        let mut base = bytes(&fixture()["registry"][0]);
        base[56] = 7; // C# deliberately does not require bytes 2/4 to be zero.
        base[58] = 7;
        base[59..72].copy_from_slice(b" \x80AB\n\r \0     ");
        base[90..108].fill(0);
        base[93] = 0xff;
        base[95..108].copy_from_slice(b" SECOND\n     ");
        let parsed = parse(&base).unwrap();
        assert_eq!(
            parsed.descriptors,
            [
                ("Model", " ?AB".into()),
                ("Serial Number", "D8K4P27".into()),
                ("Serial Number", " SECOND".into())
            ]
        );
        base[108..126].fill(0);
        base[111] = 0xff;
        base[113..126].copy_from_slice(b"LAST-SLOT-927");
        assert_eq!(
            parse(&base).unwrap().descriptors.last(),
            Some(&("Serial Number", "LAST-SLOT-927".into()))
        );
        base[108] = 1; // A detailed timing is not a descriptor, even with tag-like bytes.
        assert_eq!(parse(&base).unwrap().descriptors.len(), 3);
        base.extend_from_slice(&[0xff; 128]); // Extensions do not change base identities.
        assert_eq!(parse(&base).unwrap().descriptors, parsed.descriptors);
    }
}
