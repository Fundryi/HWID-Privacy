//! WMI RAM modules in the legacy, UTF-16-aligned table.

use crate::{
    hw::Ctx,
    report::{Out, pad_right_utf16},
    win::{self, wmi},
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
pub fn collect(_ctx: &Ctx, out: &mut Out) -> Result<(), win::Error> {
    out.source("WMI");
    // C# parity: Hardware/RamInfo.cs:66-79. Preserve WMI order and untrimmed values.
    let rows = wmi::query(wmi::Namespace::Cimv2, "SELECT * FROM Win32_PhysicalMemory")?;
    let mut modules = Vec::with_capacity(rows.len());
    let mut errors = Vec::new();
    for row in rows {
        let capacity = if row.str("Capacity").is_none() {
            // C# parity: Hardware/RamInfo.cs:70. A null Capacity converts to zero.
            Some(0)
        } else {
            row.u64("Capacity")
        };
        match capacity {
            Some(capacity) => modules.push(RamModule::new(
                row.str("DeviceLocator").unwrap_or_default(),
                row.str("Manufacturer").unwrap_or_default(),
                row.str("PartNumber").unwrap_or_default(),
                capacity,
                row.str("SerialNumber").unwrap_or_default(),
            )),
            None => errors.push(win::Error::msg(
                "Convert.ToUInt64",
                "Win32_PhysicalMemory.Capacity is not an unsigned 64-bit value",
            )),
        }
    }
    // Keep usable rows if one value cannot be converted; never report an empty
    // result as "No RAM modules" when a failed conversion caused it.
    if !modules.is_empty() || errors.is_empty() {
        write_table(&modules, out);
    }
    for error in errors {
        out.fallback_failed("WMI", &error).text(&format!(
            "Error retrieving RAM MODULES information: {error}"
        ));
    }
    Ok(())
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
        out.text("No RAM modules detected.");
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
        out.id_value(&module.fields[4]).text(&table_line(
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
        assert_eq!(section.ids, ["7C3E91A2", "", "24B7D19F    "]);
        let mut empty = Out::new();
        write_table(&[], &mut empty);
        assert_eq!(empty.finish().body, "No RAM modules detected.\r\n");
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
