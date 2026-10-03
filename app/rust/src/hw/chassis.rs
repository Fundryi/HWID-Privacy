//! Chassis identifiers and the legacy chassis type names from shared SMBIOS data.

use crate::{hw::Ctx, report::Out, win};
use win::firmware::Smbios;

/// Collects CHASSIS from SMBIOS, retaining the C# unavailable and optional-field texts.
pub fn collect(ctx: &Ctx, out: &mut Out) -> Result<(), win::Error> {
    if let Some(smbios) = ctx.smbios() {
        write_smbios(smbios, out);
    } else {
        out.text("Chassis information not available.");
        // AD-03: the C# firmware reader silently returned null on an OS/parser failure.
        ctx.smbios_result()?;
    }
    Ok(())
}

fn write_smbios(smbios: &Smbios, out: &mut Out) {
    let mut fields = [""; 4];
    let mut chassis_type = None;
    // C# parity: FirmwareTable.cs:133-134,255-259. Keep earlier fields absent from a short repeat.
    for chassis in smbios.structures.iter().filter(|record| record.kind == 3) {
        for (field, offset) in fields.iter_mut().zip([4, 6, 7, 8]) {
            if let Some(index) = chassis.byte(offset) {
                *field = chassis.string(index);
            }
        }
        if let Some(value) = chassis.byte(5) {
            chassis_type = Some(value);
        }
    }
    out.source("native");
    // C# parity: ChassisInfo.cs:23-37. A manufacturer is required; other empty fields are omitted.
    if fields[0].is_empty() {
        out.text("Chassis information not available.");
        return;
    }
    out.info("Manufacturer", fields[0]);
    if let Some(value) = chassis_type {
        out.info("Type", &decode_chassis_type(value));
    }
    for (label, value) in ["Version", "Serial Number", "Asset Tag"]
        .into_iter()
        .zip(&fields[1..])
    {
        if !value.is_empty() {
            if label == "Version" {
                out.info(label, value);
            } else {
                out.id(label, value);
            }
        }
    }
}

fn decode_chassis_type(value: u8) -> String {
    // C# parity: FirmwareTable.cs:273-305. Mask the lock bit; leave the table's gaps unmapped.
    let value = value & 0x7f;
    let name = match value {
        1 => "Other",
        2 => "Unknown",
        3 => "Desktop",
        4 => "Low Profile Desktop",
        5 => "Pizza Box",
        6 => "Mini Tower",
        7 => "Tower",
        8 => "Portable",
        9 => "Laptop",
        10 => "Notebook",
        11 => "Hand Held",
        12 => "Docking Station",
        13 => "All in One",
        14 => "Sub Notebook",
        15 => "Space-saving",
        16 => "Lunch Box",
        17 => "Main Server Chassis",
        23 => "Rack Mount Chassis",
        24 => "Sealed-case PC",
        30 => "Tablet",
        31 => "Convertible",
        32 => "Detachable",
        33 => "IoT Gateway",
        34 => "Embedded PC",
        35 => "Mini PC",
        36 => "Stick PC",
        _ => return format!("Type {value}"),
    };
    name.to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wp02_chassis_text_lock_bit_gaps_and_manufacturer_gate() {
        let raw: Vec<u8> = include_str!("../../tests/fixtures/wp-02/smbios.hex")
            .split_whitespace()
            .map(|byte| u8::from_str_radix(byte, 16).expect("fixture hex"))
            .collect();
        let mut smbios = win::firmware::parse_smbios(&raw).expect("fixture table");
        let mut repeated = smbios
            .structures
            .iter()
            .find(|r| r.kind == 3)
            .expect("fixture chassis")
            .clone();
        repeated.formatted[5] = 0x83;
        smbios.structures.push(repeated.clone());
        let mut out = Out::new();
        write_smbios(&smbios, &mut out);
        let section = out.finish();
        assert_eq!(
            section.body,
            concat!(
                "Manufacturer: Micro-Star International Co., Ltd.\r\n",
                "Type: Desktop\r\nVersion: 1.0\r\n",
                "Serial Number: CHS2410B937462\r\nAsset Tag: ASSET24100372\r\n",
            )
        );
        assert_eq!(section.ids, ["CHS2410B937462", "ASSET24100372"]);
        repeated.formatted[5] = 0x92;
        repeated.formatted[6..9].fill(0);
        smbios.structures.push(repeated.clone());
        let mut out = Out::new();
        write_smbios(&smbios, &mut out);
        assert_eq!(
            out.finish().body,
            "Manufacturer: Micro-Star International Co., Ltd.\r\nType: Type 18\r\n"
        );
        repeated.formatted.truncate(5);
        repeated.strings[0] = "  MSI  ".to_owned();
        smbios.structures.push(repeated);
        let mut out = Out::new();
        write_smbios(&smbios, &mut out);
        assert_eq!(
            out.finish().body,
            "Manufacturer:   MSI  \r\nType: Type 18\r\n"
        );
        smbios
            .structures
            .last_mut()
            .expect("repeated chassis")
            .formatted[4] = 0;
        let mut out = Out::new();
        write_smbios(&smbios, &mut out);
        assert_eq!(out.finish().body, "Chassis information not available.\r\n");
        for (code, name) in [
            (0, "Type 0"),
            (17, "Main Server Chassis"),
            (23, "Rack Mount Chassis"),
            (24, "Sealed-case PC"),
            (30, "Tablet"),
            (36, "Stick PC"),
            (127, "Type 127"),
        ] {
            assert_eq!(decode_chassis_type(code), name);
            assert_eq!(decode_chassis_type(code | 0x80), name);
        }
    }
}
