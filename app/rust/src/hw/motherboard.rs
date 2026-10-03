//! Baseboard identifiers from the shared SMBIOS snapshot, with the legacy WMI fallback.

use crate::{hw::Ctx, report::Out, win};
use win::{firmware::Smbios, wmi};

/// Collects MOTHERBOARD in the C# field order, preferring direct SMBIOS data.
pub fn collect(ctx: &Ctx, out: &mut Out) -> Result<(), win::Error> {
    let mut firmware_error = None;
    if let Some(smbios) = ctx.smbios() {
        if write_smbios(smbios, out) {
            return Ok(());
        }
    } else if let Err(error) = ctx.smbios_result() {
        out.fallback_failed("SMBIOS", &error);
        firmware_error = Some(error);
    }

    // C# parity: MotherboardInfo.cs:43-51. No Version, Source or separators in WMI output.
    let rows = wmi::query(wmi::Namespace::Cimv2, "SELECT * FROM Win32_BaseBoard")?;
    // AD-03: an empty fallback cannot hide a firmware failure behind an empty section.
    if rows.is_empty()
        && let Some(error) = firmware_error
    {
        return Err(error);
    }
    out.source("WMI");
    for row in rows {
        for label in ["Manufacturer", "Product", "Model", "SKU", "SerialNumber"] {
            let value = row.str(label).unwrap_or_default();
            if matches!(label, "SKU" | "SerialNumber") {
                out.id(label, &value);
            } else {
                out.info(label, &value);
            }
        }
    }
    Ok(())
}

fn write_smbios(smbios: &Smbios, out: &mut Out) -> bool {
    let mut fields = [""; 6];
    // C# parity: FirmwareTable.cs:127-132,239-244. Later records overwrite only present fields.
    for board in smbios.structures.iter().filter(|record| record.kind == 2) {
        for (field, offset) in fields.iter_mut().zip([4, 5, 6, 7, 8, 0x0a]) {
            if let Some(index) = board.byte(offset) {
                *field = board.string(index);
            }
        }
    }
    // C# parity: MotherboardInfo.cs:25-39. An empty manufacturer selects WMI, even with a serial.
    if fields[0].is_empty() {
        return false;
    }
    out.source("native")
        .info("Manufacturer", fields[0])
        .info("Product", fields[1])
        .info("Version", fields[2])
        .id("SerialNumber", fields[3]);
    if !fields[4].is_empty() {
        out.id("Asset Tag", fields[4]);
    }
    if !fields[5].is_empty() {
        out.info("Location", fields[5]);
    }
    out.info("Source", "SMBIOS (direct)");
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wp02_baseboard_text_optional_fields_and_repeated_records() {
        let raw: Vec<u8> = include_str!("../../tests/fixtures/wp-02/smbios.hex")
            .split_whitespace()
            .map(|byte| u8::from_str_radix(byte, 16).expect("fixture hex"))
            .collect();
        let mut smbios = win::firmware::parse_smbios(&raw).expect("fixture table");
        let mut out = Out::new();
        assert!(write_smbios(&smbios, &mut out));
        let section = out.finish();
        assert_eq!(
            section.body,
            concat!(
                "Manufacturer: Micro-Star International Co., Ltd.\r\n",
                "Product: MAG B650 TOMAHAWK WIFI\r\n",
                "Version: 1.0\r\nSerialNumber: 07D7524A0196384\r\n",
                "Asset Tag: ASSET24100371\r\nLocation: Baseboard Bay 1\r\n",
                "Source: SMBIOS (direct)\r\n",
            )
        );
        assert_eq!(section.ids, ["07D7524A0196384", "ASSET24100371"]);
        let mut repeated = smbios
            .structures
            .iter()
            .find(|r| r.kind == 2)
            .expect("fixture baseboard")
            .clone();
        repeated.strings[3] = "07D7524B0672195".to_owned();
        repeated.formatted[8] = 0;
        repeated.formatted[0x0a] = 0;
        smbios.structures.push(repeated.clone());
        let mut out = Out::new();
        assert!(write_smbios(&smbios, &mut out));
        assert_eq!(
            out.finish().body,
            concat!(
                "Manufacturer: Micro-Star International Co., Ltd.\r\n",
                "Product: MAG B650 TOMAHAWK WIFI\r\nVersion: 1.0\r\n",
                "SerialNumber: 07D7524B0672195\r\nSource: SMBIOS (direct)\r\n",
            )
        );
        repeated.formatted.truncate(6);
        repeated.strings[1] = "MS-7D75".to_owned();
        smbios.structures.push(repeated);
        let mut out = Out::new();
        assert!(write_smbios(&smbios, &mut out));
        let body = out.finish().body;
        assert!(body.contains("Product: MS-7D75\r\n"));
        assert!(body.contains("SerialNumber: 07D7524B0672195\r\n"));
        smbios
            .structures
            .last_mut()
            .expect("repeated baseboard")
            .formatted[4] = 0;
        let mut out = Out::new();
        assert!(!write_smbios(&smbios, &mut out));
        assert!(out.finish().body.is_empty());
    }
}
