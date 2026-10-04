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
    /// Whether the complete base-block byte sum is zero modulo 256.
    pub checksum_valid: bool,
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
    Ok(Edid {
        manufacturer,
        descriptors,
        numeric_serial,
        checksum_valid: bytes[..128]
            .iter()
            .fold(0_u8, |sum, byte| sum.wrapping_add(*byte))
            == 0,
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used)] // Fixture assertions may panic; production code may not.
mod tests {
    use super::*;

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
    }
}
