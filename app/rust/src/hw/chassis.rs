//! Chassis identifiers and the legacy chassis type names from shared SMBIOS data.

use crate::{hw::Ctx, report::Out, win};
use win::firmware::{Smbios, Structure};

/// Collects CHASSIS from SMBIOS, retaining the C# unavailable and optional-field texts.
pub fn collect(ctx: &Ctx, out: &mut Out) -> Result<(), win::Error> {
    if let Some(smbios) = ctx.smbios() {
        write_smbios(smbios, out);
        append_power_supplies(smbios, out);
    } else {
        out.text("Chassis information not available.");
        // AD-03: the C# firmware reader silently returned null on an OS/parser failure.
        ctx.smbios_result()?;
    }
    Ok(())
}

fn append_power_supplies(smbios: &Smbios, out: &mut Out) {
    let mut supplies = win::firmware::power_supplies(smbios);
    supplies.retain(|supply| {
        super::bios::metadata_record_has_values(
            out,
            "Power Supply",
            None,
            &[
                &supply.manufacturer,
                &supply.model,
                &supply.revision,
                &supply.serial,
                &supply.asset,
            ],
        )
    });
    for (index, supply) in supplies.iter().enumerate() {
        if supplies.len() >= 2 {
            out.text(&format!("Power Supply #{} (SMBIOS)", index + 1));
        }
        for (label, identity, value) in [
            ("Manufacturer (SMBIOS)", false, &supply.manufacturer),
            ("Model/Part (SMBIOS)", false, &supply.model),
            ("Revision (SMBIOS)", false, &supply.revision),
            ("Serial (SMBIOS)", true, &supply.serial),
            ("Asset Tag (SMBIOS)", true, &supply.asset),
        ] {
            super::bios::write_metadata_field(out, label, identity, value);
        }
    }
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
    // Associate the new SKU with the last chassis rather than borrowing one from
    // an earlier record when a short repeat supplied the legacy primary fields.
    if (smbios.major, smbios.minor) >= (2, 7)
        && let Some(chassis) = smbios
            .structures
            .iter()
            .rev()
            .find(|record| record.kind == 3)
        && let Some(sku) = chassis_sku(chassis)
        && super::bios::useful_new_value(sku)
        && !fields.contains(&sku)
    {
        out.id("SKU", sku);
    }
}

fn chassis_sku(chassis: &Structure) -> Option<&str> {
    // DSP0134 7.4: SKU follows n contained elements, each m bytes long.
    let count = usize::from(chassis.byte(0x13)?);
    let width = usize::from(chassis.byte(0x14)?);
    if count > 0 && width < 3 {
        return None;
    }
    chassis
        .byte(0x15 + count * width)
        .map(|index| chassis.string(index))
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
    fn power_supply_rendering_counts_only_printable_records_and_marks_unit_values() {
        let empty = Structure {
            kind: 39,
            handle: 0x3901,
            formatted: vec![0; 12],
            strings: vec!["To Be Filled By O.E.M.".into()],
        };
        let mut supply = empty.clone();
        supply.formatted[7..12].copy_from_slice(&[1, 2, 3, 4, 0]);
        supply.strings = [
            "Delta Electronics",
            "PSU2418K73196",
            "INV-PSU-2418",
            "DPS-750AB-12",
        ]
        .map(String::from)
        .to_vec();
        let mut table = Smbios {
            major: 3,
            minor: 6,
            structures: vec![empty.clone()],
        };
        table.structures[0].formatted[7..12].fill(1);
        let mut out = Out::new();
        append_power_supplies(&table, &mut out);
        let section = out.finish();
        assert!(section.body.is_empty());
        assert_eq!(section.failures.len(), 1);
        table.structures.push(supply.clone());
        let mut out = Out::new();
        append_power_supplies(&table, &mut out);
        let single = out.finish();
        assert!(
            single
                .body
                .starts_with("Manufacturer (SMBIOS): Delta Electronics\r\n")
        );
        assert!(!single.body.contains("Power Supply"));
        assert_eq!(single.ids, ["PSU2418K73196", "INV-PSU-2418"]);
        supply.formatted[8] = 255;
        table.structures.push(supply);
        let mut out = Out::new();
        append_power_supplies(&table, &mut out);
        let multiple = out.finish();
        assert!(multiple.body.starts_with("Power Supply #1 (SMBIOS)\r\n"));
        assert!(multiple.body.contains("Power Supply #2 (SMBIOS)\r\n"));
        assert!(!multiple.body.contains("Power Supply #3"));
        assert!(multiple.body.contains("Serial (SMBIOS): Unavailable ("));
        assert!(
            !crate::report::masked(&multiple)
                .body
                .contains("PSU2418K73196")
        );
    }

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
                "SKU: Desktop Chassis\r\n",
            )
        );
        assert_eq!(
            section.ids,
            ["CHS2410B937462", "ASSET24100372", "Desktop Chassis"]
        );
        // Outside-format bounds: SKU must follow the entire element array.
        let mut variable = repeated.clone();
        variable.formatted[0x13] = 2;
        variable
            .formatted
            .splice(0x15..0x15, [0x82, 1, 1, 0x83, 1, 1]);
        assert_eq!(chassis_sku(&variable), Some("Desktop Chassis"));
        for end in 0..variable.formatted.len() {
            let mut short = variable.clone();
            short.formatted.truncate(end);
            assert_eq!(chassis_sku(&short), None);
        }
        variable.formatted[0x14] = 0;
        assert_eq!(chassis_sku(&variable), None);
        variable.formatted[0x14] = 255;
        assert_eq!(chassis_sku(&variable), None);
        repeated.formatted[0x15] = 0;
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
