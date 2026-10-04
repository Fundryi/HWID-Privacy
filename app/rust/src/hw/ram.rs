//! SMBIOS RAM modules with WMI fallback in the legacy, UTF-16-aligned table.

use crate::{
    hw::Ctx,
    report::{Out, pad_right_utf16},
    win::{
        self,
        firmware::{Smbios, Structure},
        wmi,
    },
};

struct RamModule {
    fields: [String; 5],
}

impl RamModule {
    fn new(
        locator: String,
        manufacturer: String,
        part_number: String,
        capacity_bytes: u64,
        serial_number: String,
    ) -> Self {
        Self {
            fields: [
                locator,
                manufacturer,
                part_number,
                format_capacity(capacity_bytes),
                serial_number,
            ],
        }
    }
}

/// Collects this hardware section through the shared output builder.
pub fn collect(ctx: &Ctx, out: &mut Out) -> Result<(), win::Error> {
    collect_modules(ctx.smbios_result(), wmi_modules, out)
}

fn collect_modules(
    snapshot: Result<&Smbios, win::Error>,
    fallback: impl FnOnce() -> Result<Vec<RamModule>, win::Error>,
    out: &mut Out,
) -> Result<(), win::Error> {
    let direct = match snapshot {
        Ok(smbios) => smbios_modules(smbios),
        Err(error) => {
            out.fallback_failed("SMBIOS", &error);
            Vec::new()
        }
    };
    if !direct.is_empty() && direct.iter().flatten().all(Option::is_some) {
        out.source("SMBIOS");
        let modules: Vec<_> = direct
            .into_iter()
            .map(|fields| RamModule {
                // The completeness check above proves every field is present.
                fields: fields.map(Option::unwrap_or_default),
            })
            .collect();
        write_table(&modules, out);
        return Ok(());
    }

    out.source("WMI");
    let mut modules = fallback()?;
    // WMI owns inventory and order on the incomplete-firmware path. Never join
    // by position: empty slots and missing records can shift SMBIOS indexes.
    let matches: Vec<_> = modules
        .iter()
        .map(|module| {
            [0, 4].into_iter().find_map(|key| {
                let value = &module.fields[key];
                if value.trim().is_empty()
                    || modules
                        .iter()
                        .filter(|row| row.fields[key] == *value)
                        .count()
                        != 1
                {
                    return None;
                }
                let mut matches = direct
                    .iter()
                    .enumerate()
                    .filter(|(_, row)| row[key].as_ref() == Some(value));
                let (index, row) = matches.next()?;
                if matches.next().is_some()
                    || (key == 4
                        && row[0].as_ref().is_some_and(|locator| {
                            !module.fields[0].is_empty() && *locator != module.fields[0]
                        }))
                {
                    return None;
                }
                Some(index)
            })
        })
        .collect();
    for (module, matching) in modules.iter_mut().zip(&matches) {
        if let Some(index) =
            matching.filter(|_| matches.iter().filter(|other| *other == matching).count() == 1)
        {
            for (value, direct) in module.fields.iter_mut().zip(&direct[index]) {
                if let Some(direct) = direct {
                    value.clone_from(direct);
                }
            }
            out.source("SMBIOS + WMI");
        }
    }
    write_table(&modules, out);
    Ok(())
}

fn smbios_modules(smbios: &Smbios) -> Vec<[Option<String>; 5]> {
    smbios
        .structures
        .iter()
        .filter(|record| record.kind == 17 && record.word(0x0c) != Some(0))
        .map(|record| {
            let string = |offset| {
                let value = record.string(record.byte(offset)?);
                // Test for absence without changing the firmware's padding.
                (!value.trim().is_empty()).then(|| value.to_owned())
            };
            let capacity = capacity_bytes(record).map(format_capacity);
            [
                string(0x10),
                string(0x17),
                string(0x1a),
                capacity,
                string(0x18),
            ]
        })
        .collect()
}

fn capacity_bytes(record: &Structure) -> Option<u64> {
    // DSP0134 type 17: zero is an empty slot, FFFF is unknown;
    // 7FFF selects the 31-bit extended MB count. Otherwise bit 15
    // selects KB instead of MB and is not part of the size.
    match record.word(0x0c)? {
        0 | 0xffff => None,
        0x7fff => record
            .dword(0x1c)
            .map(|size| u64::from(size & 0x7fff_ffff) * 1_048_576),
        size => Some(u64::from(size & 0x7fff) * if size & 0x8000 != 0 { 1024 } else { 1_048_576 }),
    }
    .filter(|bytes| *bytes != 0)
}

fn wmi_modules() -> Result<Vec<RamModule>, win::Error> {
    // C# parity: Hardware/RamInfo.cs:66-79. Preserve WMI order and untrimmed values.
    let rows = wmi::query(wmi::Namespace::Cimv2, "SELECT * FROM Win32_PhysicalMemory")?;
    let mut modules = Vec::with_capacity(rows.len());
    for row in rows {
        let capacity = if row.str("Capacity").is_none() {
            // C# parity: Hardware/RamInfo.cs:70. A null Capacity converts to zero.
            0
        } else {
            row.u64("Capacity").ok_or_else(|| {
                win::Error::msg(
                    "Convert.ToUInt64",
                    "Win32_PhysicalMemory.Capacity is not an unsigned 64-bit value",
                )
            })?
        };
        modules.push(RamModule::new(
            row.str("DeviceLocator").unwrap_or_default(),
            row.str("Manufacturer").unwrap_or_default(),
            row.str("PartNumber").unwrap_or_default(),
            capacity,
            row.str("SerialNumber").unwrap_or_default(),
        ));
    }
    // C# parity: Hardware/RamInfo.cs:68-79. Convert every row before printing the table.
    Ok(modules)
}

fn format_capacity(bytes: u64) -> String {
    // C# parity: Hardware/RamInfo.cs:71,78. N0 uses invariant grouping and
    // ties-to-even rounding of the double, including the legacy "GB" label.
    let digits = ((bytes as f64 / 1_073_741_824.0).round_ties_even() as u64).to_string();
    let mut grouped = String::with_capacity(digits.len() + digits.len() / 3 + 3);
    for (index, digit) in digits.chars().enumerate() {
        if index != 0 && (digits.len() - index).is_multiple_of(3) {
            grouped.push(',');
        }
        grouped.push(digit);
    }
    grouped.push_str(" GB");
    grouped
}

fn write_table(modules: &[RamModule], out: &mut Out) {
    // C# parity: Hardware/RamInfo.cs:29. The empty result has no table header.
    if modules.is_empty() {
        out.text("No RAM modules detected.").trim_end();
        return;
    }
    // C# parity: Hardware/RamInfo.cs:32-46. Widths use data, not the headers.
    let mut widths = [15, 12, 10, 8, 12];
    for module in modules {
        for (width, value) in widths.iter_mut().zip(&module.fields) {
            *width = (*width).max(value.encode_utf16().count());
        }
    }
    let header = [
        "DeviceLocator",
        "Manufacturer",
        "PartNumber",
        "Capacity",
        "SerialNumber",
    ];
    out.text(&table_line(header, widths))
        .text(&"-".repeat(widths.iter().sum::<usize>() + 4));
    // C# parity: Hardware/RamInfo.cs:49-57. Keep padding on the last column.
    for module in modules {
        if !module.fields[4].is_empty() {
            out.id_value(&module.fields[4]);
        }
        out.text(&table_line(
            module.fields.each_ref().map(String::as_str),
            widths,
        ));
    }
}

fn table_line(fields: [&str; 5], widths: [usize; 5]) -> String {
    fields
        .into_iter()
        .zip(widths)
        .map(|(value, width)| pad_right_utf16(value, width))
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Deserialize;

    #[derive(Deserialize)]
    #[serde(rename_all = "PascalCase")]
    struct FixtureModule {
        device_locator: Option<String>,
        manufacturer: Option<String>,
        part_number: Option<String>,
        capacity: Option<u64>,
        serial_number: Option<String>,
    }

    #[derive(Deserialize)]
    struct FirmwareFixture {
        strings: Vec<String>,
        cases: Vec<FirmwareCase>,
    }

    #[derive(Deserialize)]
    struct FirmwareCase {
        name: String,
        formatted_hex: String,
        capacity_bytes: Option<u64>,
        populated: bool,
    }

    #[test]
    fn fabricated_ram_table_matches_legacy_bytes_and_serial_records() {
        let rows: Vec<FixtureModule> =
            serde_json::from_str(include_str!("../../tests/fixtures/wp-04/ram-wmi.json"))
                .expect("fabricated RAM fixture");
        let modules: Vec<_> = rows
            .into_iter()
            .map(|row| {
                RamModule::new(
                    row.device_locator.unwrap_or_default(),
                    row.manufacturer.unwrap_or_default(),
                    row.part_number.unwrap_or_default(),
                    row.capacity.unwrap_or_default(),
                    row.serial_number.unwrap_or_default(),
                )
            })
            .collect();
        let mut out = Out::new();
        write_table(&modules, &mut out);
        let section = out.finish();
        let expected: String =
            serde_json::from_str(include_str!("../../tests/fixtures/wp-04/ram-table.json"))
                .expect("legacy RAM text with escaped CRLF");
        assert_eq!(section.body.as_bytes(), expected.as_bytes());
        assert_eq!(section.ids, ["7C3E91A2", "24B7D19F    "]);
        let fixture: FirmwareFixture =
            serde_json::from_str(include_str!("../../tests/fixtures/wp-04/ram-smbios.json"))
                .expect("fabricated type 17 encodings");
        let mut smbios = Smbios {
            major: 3,
            minor: 2,
            structures: Vec::new(),
        };
        for case in fixture.cases {
            let record = Structure {
                kind: 17,
                handle: 0,
                formatted: case
                    .formatted_hex
                    .split_whitespace()
                    .map(|byte| u8::from_str_radix(byte, 16).expect("hex byte"))
                    .collect(),
                strings: fixture.strings.clone(),
            };
            assert_eq!(
                capacity_bytes(&record),
                case.capacity_bytes,
                "{}",
                case.name
            );
            smbios.structures = vec![record];
            let direct = smbios_modules(&smbios);
            assert_eq!(direct.len(), usize::from(case.populated), "{}", case.name);
            if case.name == "16 GiB MB size" {
                let mut out = Out::new();
                collect_modules(
                    Ok(&smbios),
                    || panic!("complete firmware must skip WMI"),
                    &mut out,
                )
                .expect("direct collection");
                assert_eq!(out.finish().source, "SMBIOS");
                assert_eq!(direct[0][0].as_deref(), Some("DIMM_A2  "));
                assert_eq!(direct[0][2].as_deref(), Some("KSM32RD8/32HCR   "));
                assert_eq!(direct[0][4].as_deref(), Some("7C3E91A2  "));
            }
        }
        // The last fixture has an invalid part-number index: keep WMI's value
        // for that field, even with reversed inventory and an unrelated row.
        let fallback = || {
            Ok(vec![
                RamModule::new(
                    "OTHER".into(),
                    "Vendor".into(),
                    "Other part".into(),
                    0,
                    "OTHER-SN".into(),
                ),
                RamModule::new(
                    "DIMM_A2  ".into(),
                    "WMI vendor".into(),
                    "WMI part".into(),
                    0,
                    "WMI-SN".into(),
                ),
            ])
        };
        let mut out = Out::new();
        collect_modules(Ok(&smbios), fallback, &mut out).expect("field fallback");
        let mixed = out.finish();
        assert_eq!(mixed.source, "SMBIOS + WMI");
        assert!(mixed.body.contains("WMI part"));
        assert!(!mixed.body.contains("WMI vendor"));
        assert_eq!(mixed.ids, ["OTHER-SN", "7C3E91A2  "]);
        // A missing locator can use a unique serial, retaining WMI's locator.
        smbios.structures[0].formatted[0x10] = 0;
        let mut out = Out::new();
        collect_modules(
            Ok(&smbios),
            || {
                let mut rows = fallback()?;
                rows[1].fields[4] = "7C3E91A2  ".into();
                Ok(rows)
            },
            &mut out,
        )
        .expect("serial association");
        let mixed = out.finish();
        assert_eq!(mixed.source, "SMBIOS + WMI");
        assert!(mixed.body.contains("DIMM_A2  "));
        assert!(!mixed.body.contains("WMI vendor"));
        smbios.structures[0].formatted[0x10] = 1;
        // Duplicate locators must not attach another module's identity.
        smbios.structures.push(smbios.structures[0].clone());
        let mut expected_fallback = Out::new();
        write_table(&fallback().expect("rows"), &mut expected_fallback);
        let expected_fallback = expected_fallback.finish().body;
        let missing = Smbios {
            major: 3,
            minor: 2,
            structures: Vec::new(),
        };
        for snapshot in [
            Ok(&smbios),
            Ok(&missing),
            Err(win::Error::msg("RSMB", "unavailable")),
        ] {
            let mut out = Out::new();
            collect_modules(snapshot, fallback, &mut out).expect("whole row fallback");
            assert_eq!(out.finish().body, expected_fallback);
        }
        // An unavailable fallback remains an error, never a no-modules report.
        let mut out = Out::new();
        assert!(
            collect_modules(
                Ok(&smbios),
                || Err(win::Error::msg("WMI", "denied")),
                &mut out
            )
            .is_err()
        );
        assert!(out.finish().body.is_empty());
        let mut empty = Out::new();
        write_table(&[], &mut empty);
        assert_eq!(empty.finish().body, "No RAM modules detected.");
    }

    #[test]
    fn invariant_n0_capacity_keeps_binary_units_grouping_and_even_midpoints() {
        for (bytes, expected) in [
            (0, "0 GB"),
            (536_870_912, "0 GB"),
            (1_610_612_736, "2 GB"),
            (2_684_354_560, "2 GB"),
            (3_758_096_384, "4 GB"),
            (1_099_511_627_776, "1,024 GB"),
            (u64::MAX, "17,179,869,184 GB"),
        ] {
            assert_eq!(format_capacity(bytes), expected);
        }
    }

    #[test]
    #[ignore = "read-only owner-PC RAM capture; redirect output to private golden/wp-04"]
    fn wp04_capture_ram() {
        let sections = crate::hw::collect_all(Some("RAM MODULES"), &|_, _| {});
        let section = sections.first().expect("RAM provider is registered");
        println!("WP04_CAPTURE_BEGIN RAM MODULES");
        print!("{}", section.body);
        println!("WP04_CAPTURE_END RAM MODULES");
        println!(
            "elapsed_ms={} source={} failures={:?}",
            section.elapsed_ms, section.source, section.failures
        );
        assert!(section.failures.is_empty(), "RAM collection failed");
    }
}
