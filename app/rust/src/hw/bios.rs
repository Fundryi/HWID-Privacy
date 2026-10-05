//! Direct BIOS/system identifiers enriched by independently fallible WMI queries.

use crate::{hw::Ctx, report::Out, win};
use win::{firmware::Smbios, wmi};

/// Collects (SM)BIOS, preserving direct data when either WMI enrichment query fails.
pub fn collect(ctx: &Ctx, out: &mut Out) -> Result<(), win::Error> {
    let smbios = ctx.smbios();
    let mut firmware_error = None;
    if smbios.is_none()
        && let Err(error) = ctx.smbios_result()
    {
        out.fallback_failed("SMBIOS", &error);
        firmware_error = Some(error);
    }
    let has_information = collect_with(smbios, out, |class, names| {
        wmi::query(
            wmi::Namespace::Cimv2,
            &format!("SELECT {} FROM {class}", names.join(", ")),
        )
    });
    if let Some(smbios) = smbios {
        append_components(smbios, out);
    }
    // AD-03: show the firmware failure when WMI supplies no replacement values.
    if !has_information && let Some(error) = firmware_error {
        return Err(error);
    }
    Ok(())
}

fn append_components(smbios: &Smbios, out: &mut Out) {
    let mut components = win::firmware::firmware_components(smbios);
    components.retain(|component| {
        metadata_record_has_values(
            out,
            "Firmware Component",
            Some(component.handle),
            &[
                &component.name,
                &component.version,
                &component.id,
                &component.date,
            ],
        )
    });
    for (index, component) in components.iter().enumerate() {
        if components.len() >= 2 {
            out.text(&format!("Firmware Component #{} (SMBIOS)", index + 1));
        }
        for (label, value) in [
            ("Component Name (SMBIOS)", &component.name),
            ("Component Version (SMBIOS)", &component.version),
            ("Component ID (SMBIOS)", &component.id),
            ("Component Release Date (SMBIOS)", &component.date),
        ] {
            // SMBIOS firmware IDs identify components/releases, not individual machines.
            write_metadata_field(out, label, false, value);
        }
    }
}

fn collect_with(
    smbios: Option<&Smbios>,
    out: &mut Out,
    mut query: impl FnMut(&str, &[&str]) -> win::Result<Vec<wmi::Row>>,
) -> bool {
    let mut failures = Vec::new();
    let (fields, uuid) = smbios_fields(smbios);
    let bios = query_fields(
        "Win32_BIOS",
        [
            "Manufacturer",
            "Version",
            "SMBIOSBIOSVersion",
            "SerialNumber",
        ],
        [fields[0].is_empty(), fields[1].is_empty(), true, true],
        &mut query,
        &mut failures,
    );
    let product = query_fields(
        "Win32_ComputerSystemProduct",
        ["Vendor", "UUID", "IdentifyingNumber"],
        [true, uuid.is_empty(), true],
        &mut query,
        &mut failures,
    );
    // PLAN WP-02 / AD-46: a failed query leaves only its own fields empty.
    let source = match (smbios.is_some(), failures.len() < 2) {
        (true, true) => Some("native + WMI"),
        (true, false) => Some("native"),
        (false, true) => Some("WMI"),
        (false, false) => None,
    };
    if let Some(source) = source {
        out.source(source);
    }
    write_information(smbios, &bios, &product, out);
    let has_information =
        smbios.is_some() || bios.iter().chain(&product).any(|value| !value.is_empty());
    // AD-10: one trailing error line per failed query, after all normal fields.
    for (class, error) in failures {
        out.fallback_failed(class, &error)
            .text(&format!("WMI query failed: {class}: {error}"));
    }
    has_information
}

fn query_fields<const N: usize>(
    class: &'static str,
    names: [&str; N],
    needed: [bool; N],
    query: &mut impl FnMut(&str, &[&str]) -> win::Result<Vec<wmi::Row>>,
    failures: &mut Vec<(&'static str, win::Error)>,
) -> [String; N] {
    // Only omit a fallback property when the existing renderer already prefers SMBIOS.
    // The four WMI-only properties remain authoritative, including on query failure.
    let selected: Vec<_> = names
        .iter()
        .zip(needed)
        .filter_map(|(&name, needed)| needed.then_some(name))
        .collect();
    match query(class, &selected) {
        // C# parity: BiosInfo.cs:30-47. Later WMI rows overwrite even null/empty properties.
        Ok(rows) => names.map(|name| {
            rows.last()
                .and_then(|row| row.str(name))
                .unwrap_or_default()
        }),
        Err(error) => {
            failures.push((class, error));
            std::array::from_fn(|_| String::new())
        }
    }
}

fn smbios_fields(smbios: Option<&Smbios>) -> ([&str; 8], String) {
    let mut fields = [""; 8];
    let mut uuid = String::new();
    if let Some(smbios) = smbios {
        // C# parity: FirmwareTable.cs:124-129,198-227. Later present fields overwrite earlier ones.
        for record in &smbios.structures {
            let (target, offsets): (&mut [&str], &[usize]) = match record.kind {
                0 => (&mut fields[..3], &[4, 5, 8]),
                1 => (&mut fields[3..], &[4, 5, 7, 0x19, 0x1a]),
                _ => continue,
            };
            for (field, &offset) in target.iter_mut().zip(offsets) {
                if let Some(index) = record.byte(offset) {
                    *field = record.string(index);
                }
            }
            if record.kind == 1
                && let Some(bytes) = record.formatted.get(8..0x18)
            {
                // C# parity: FirmwareTable.cs:218-223,262-270. All versions and sentinel UUIDs swap.
                uuid = format!(
                    "{:02X}{:02X}{:02X}{:02X}-{:02X}{:02X}-{:02X}{:02X}-{:02X}{:02X}-{:02X}{:02X}{:02X}{:02X}{:02X}{:02X}",
                    bytes[3],
                    bytes[2],
                    bytes[1],
                    bytes[0],
                    bytes[5],
                    bytes[4],
                    bytes[7],
                    bytes[6],
                    bytes[8],
                    bytes[9],
                    bytes[10],
                    bytes[11],
                    bytes[12],
                    bytes[13],
                    bytes[14],
                    bytes[15],
                );
            }
        }
    }
    (fields, uuid)
}

fn write_information(
    smbios: Option<&Smbios>,
    bios: &[String; 4],
    product: &[String; 3],
    out: &mut Out,
) {
    let (fields, uuid) = smbios_fields(smbios);
    let prefer_direct = |direct: &str, fallback: &str| {
        if direct.is_empty() {
            fallback.to_owned()
        } else {
            direct.to_owned()
        }
    };
    // C# parity: BiosInfo.cs:50-78. No trimming; four fields remain WMI-only.
    out.info("Manufacturer", &prefer_direct(fields[0], &bios[0]))
        .info("Vendor", &product[0])
        .info("Version", &prefer_direct(fields[1], &bios[1]))
        .info("BIOS Version", &bios[2]);
    if !fields[2].is_empty() {
        out.info("Release Date", fields[2]);
    }
    out.id("UUID", &prefer_direct(&uuid, &product[1]))
        .id("IdentifyingNumber", &product[2])
        .id("SerialNumber", &bios[3]);
    for (label, value) in [
        "System Manufacturer",
        "System Product",
        "System Serial",
        "System SKU",
        "System Family",
    ]
    .into_iter()
    .zip(&fields[3..])
    {
        if !value.is_empty() {
            if matches!(label, "System Serial" | "System SKU") {
                out.id(label, value);
            } else {
                out.info(label, value);
            }
        }
    }
    if let Some(smbios) = smbios {
        let version = smbios.structures.iter().rev().find_map(|record| {
            (record.kind == 1)
                .then(|| record.byte(6).map(|index| record.string(index)))
                .flatten()
        });
        if let Some(version) = version.filter(|value| useful_new_value(value)) {
            out.info("System Version", version);
        }
        // Type 11 is free-form OEM text, not necessarily an identifier. Only admit
        // explicit identifier labels; do not promote arbitrary vendor messages.
        for record in smbios.structures.iter().filter(|record| record.kind == 11) {
            for index in 1..=record.byte(4).unwrap_or_default() {
                let text = record.string(index);
                if text.chars().any(char::is_control) {
                    continue;
                }
                let Some((label, value)) = text.split_once([':', '=']) else {
                    continue;
                };
                if !matches!(
                    label.trim().to_ascii_lowercase().as_str(),
                    "serial"
                        | "serial number"
                        | "serialnumber"
                        | "s/n"
                        | "sn"
                        | "asset tag"
                        | "sku"
                        | "part number"
                        | "p/n"
                ) || !useful_new_value(value)
                    || fields
                        .iter()
                        .copied()
                        .chain(bios.iter().map(String::as_str))
                        .any(|existing| existing.trim() == value.trim())
                    || product
                        .iter()
                        .any(|existing| existing.trim() == value.trim())
                    || uuid == value.trim()
                {
                    continue;
                }
                out.info(
                    &format!("OEM String (0x{:04X}, {index})", record.handle),
                    text,
                )
                .id_value(value.trim());
            }
        }
    }
}

/// Appends independent optional firmware fields without changing legacy output.
pub(super) fn metadata_record_has_values(
    out: &mut Out,
    kind: &str,
    handle: Option<u16>,
    fields: &[&win::Result<Option<String>>],
) -> bool {
    let printable = fields
        .iter()
        .any(|field| matches!(field, Ok(Some(value)) if useful_new_value(value)));
    if !printable {
        let source = match handle {
            Some(handle) => format!("{kind} (SMBIOS handle 0x{handle:04X})"),
            None => format!("{kind} (SMBIOS)"),
        };
        out.fallback_failed(
            &source,
            &win::Error::msg(
                "SMBIOS metadata",
                if fields.iter().any(|field| field.is_err()) {
                    "malformed: record has no printable fields; field decoding failed"
                } else {
                    "absent or placeholder: record has no printable fields"
                },
            ),
        );
    }
    printable
}

/// Keeps independent malformed-field statuses in otherwise printable records.
pub(super) fn write_metadata_field(
    out: &mut Out,
    label: &str,
    identity: bool,
    value: &win::Result<Option<String>>,
) {
    match value {
        Ok(Some(value)) if useful_new_value(value) => {
            if identity {
                out.id(label, value);
            } else {
                out.info(label, value);
            }
        }
        Err(error) => {
            out.fallback_failed(label, error)
                .info(label, &format!("Unavailable ({error})"));
        }
        Ok(Some(_)) => {
            out.fallback_failed(
                label,
                &win::Error::msg(
                    "SMBIOS metadata",
                    "placeholder or malformed: optional field omitted",
                ),
            );
        }
        Ok(None) => {
            out.fallback_failed(
                label,
                &win::Error::msg(
                    "SMBIOS metadata",
                    "absent or unsupported: optional field omitted",
                ),
            );
        }
    }
}

pub(super) fn write_unique_metadata_field(
    out: &mut Out,
    label: &str,
    value: &win::Result<Option<String>>,
    values: &[&win::Result<Option<String>>],
) {
    if let Ok(Some(text)) = value
        && text.chars().all(|character| text.starts_with(character))
    {
        out.fallback_failed(
            label,
            &win::Error::msg(
                "SMBIOS identity",
                "implausible: repeated-character identity",
            ),
        );
        return;
    }
    if let Ok(Some(text)) = value
        && values
            .iter()
            .filter(|other| matches!(other, Ok(Some(other)) if other == text && useful_new_value(other)))
            .count()
            > 1
    {
        out.fallback_failed(
            label,
            &win::Error::msg(
                "SMBIOS identity",
                "implausible: same identity on distinct records",
            ),
        );
        return;
    }
    write_metadata_field(out, label, true, value);
}

/// Filters only optional new firmware fields; legacy values remain byte-identical.
pub(super) fn useful_new_value(value: &str) -> bool {
    if value.chars().any(char::is_control) {
        return false;
    }
    let value = value.trim();
    !value.is_empty()
        && !matches!(
            value.to_ascii_lowercase().as_str(),
            "default string"
                | "to be filled by o.e.m."
                | "to be filled by oem"
                | "not specified"
                | "not applicable"
                | "not available"
                | "unspecified"
                | "unknown"
                | "none"
                | "n/a"
                | "system version"
                | "system sku"
                | "system family"
                | "system serial number"
                | "chassis serial number"
                | "no asset tag"
        )
        && !value
            .chars()
            .all(|c| matches!(c, '0' | 'f' | 'F' | '-' | ' '))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn component_rendering_omits_empty_handles_and_retains_independent_context() {
        let mut record = win::firmware::Structure {
            kind: 45,
            handle: 0x4501,
            formatted: vec![0; 10],
            strings: vec!["System Firmware".into(), "2802".into()],
        };
        let mut table = Smbios {
            major: 3,
            minor: 6,
            structures: vec![record.clone()],
        };
        let mut out = Out::new();
        append_components(&table, &mut out);
        let empty = out.finish();
        assert!(empty.body.is_empty());
        assert_eq!(empty.failures.len(), 1);
        assert!(empty.failures[0].contains("0x4501"));
        record.formatted[4] = 1;
        record.formatted[5] = 2;
        record.formatted[7] = 255;
        table.structures.push(record.clone());
        let mut out = Out::new();
        append_components(&table, &mut out);
        let single = out.finish();
        assert!(
            single
                .body
                .starts_with("Component Name (SMBIOS): System Firmware\r\n")
        );
        assert!(single.body.contains("Component ID (SMBIOS): Unavailable ("));
        assert!(!single.body.contains("0x4501"));
        assert!(single.ids.is_empty());
        table.structures.push(record);
        let mut out = Out::new();
        append_components(&table, &mut out);
        let multiple = out.finish();
        assert!(
            multiple
                .body
                .starts_with("Firmware Component #1 (SMBIOS)\r\n")
        );
        assert!(multiple.body.contains("Firmware Component #2 (SMBIOS)\r\n"));
    }

    fn fixture() -> Smbios {
        let raw: Vec<u8> = include_str!("../../tests/fixtures/wp-02/smbios.hex")
            .split_whitespace()
            .map(|byte| u8::from_str_radix(byte, 16).expect("fixture hex"))
            .collect();
        win::firmware::parse_smbios(&raw).expect("fixture table")
    }

    #[test]
    fn wp02_bios_text_wmi_authority_uuid_versions_and_repeated_records() {
        let mut smbios = fixture();
        let bios = ["WMI Manufacturer", "WMI Version", "1.E1", "BIOS2410G0936"].map(String::from);
        let product = [
            "WMI Vendor",
            "78563A12-9ABC-DEF0-8245-6789ABCDEF11",
            "PRD2410A7216",
        ]
        .map(String::from);
        let mut out = Out::new();
        write_information(Some(&smbios), &bios, &product, &mut out);
        let section = out.finish();
        assert_eq!(
            section.body,
            concat!(
                "Manufacturer: American Megatrends International LLC.\r\n",
                "Vendor: WMI Vendor\r\nVersion: 1.E2\r\nBIOS Version: 1.E1\r\n",
                "Release Date: 08/14/2025\r\nUUID: 78563A12-9ABC-DEF0-8245-6789ABCDEF10\r\n",
                "IdentifyingNumber: PRD2410A7216\r\nSerialNumber: BIOS2410G0936\r\n",
                "System Manufacturer: Micro-Star International Co., Ltd.\r\n",
                "System Product: MS-7D75\r\nSystem Serial: SYS2410D5A7286\r\n",
                "System SKU: SKU-B650-042\r\nSystem Family: Desktop Family\r\n",
                "System Version: 1.0\r\n",
            )
        );
        assert_eq!(
            section.ids,
            [
                "78563A12-9ABC-DEF0-8245-6789ABCDEF10",
                "PRD2410A7216",
                "BIOS2410G0936",
                "SYS2410D5A7286",
                "SKU-B650-042"
            ]
        );
        smbios.major = 2;
        smbios.minor = 5;
        let mut out = Out::new();
        write_information(Some(&smbios), &bios, &product, &mut out);
        assert_eq!(out.finish().body, section.body);

        let mut repeated = smbios
            .structures
            .iter()
            .find(|r| r.kind == 1)
            .expect("fixture system")
            .clone();
        for (byte, expected) in [
            (0, "00000000-0000-0000-0000-000000000000"),
            (255, "FFFFFFFF-FFFF-FFFF-FFFF-FFFFFFFFFFFF"),
        ] {
            repeated.formatted[8..0x18].fill(byte);
            smbios.structures.push(repeated.clone());
            let mut out = Out::new();
            write_information(Some(&smbios), &bios, &product, &mut out);
            let section = out.finish();
            assert!(
                section
                    .body
                    .contains(&format!("UUID: {expected} (placeholder)\r\n"))
            );
            assert!(section.ids.iter().any(|id| id == expected));
            assert!(
                crate::report::masked(&section)
                    .body
                    .contains("UUID: XXXXXXXX-XXXX-XXXX-XXXX-XXXXXXXXXXXX (placeholder)\r\n")
            );
        }
        repeated.formatted.truncate(8);
        repeated.formatted[7] = 0;
        smbios.structures.push(repeated);
        let mut out = Out::new();
        write_information(Some(&smbios), &bios, &product, &mut out);
        let body = out.finish().body;
        assert!(body.contains("UUID: FFFFFFFF-FFFF-FFFF-FFFF-FFFFFFFFFFFF (placeholder)\r\n"));
        assert!(!body.contains("System Serial:"));
        assert!(body.contains("System SKU: SKU-B650-042\r\n"));
        let mut out = Out::new();
        write_information(None, &bios, &product, &mut out);
        assert_eq!(
            out.finish().body,
            concat!(
                "Manufacturer: WMI Manufacturer\r\nVendor: WMI Vendor\r\nVersion: WMI Version\r\n",
                "BIOS Version: 1.E1\r\nUUID: 78563A12-9ABC-DEF0-8245-6789ABCDEF11\r\n",
                "IdentifyingNumber: PRD2410A7216\r\nSerialNumber: BIOS2410G0936\r\n",
            )
        );
        let mut smbios = fixture();
        let raw: Vec<u8> = include_str!("../../tests/fixtures/wp-02/oem-identifiers.hex")
            .split_whitespace()
            .map(|byte| u8::from_str_radix(byte, 16).expect("fixture hex"))
            .collect();
        smbios.structures.extend(
            win::firmware::parse_smbios(&raw)
                .expect("OEM fixture")
                .structures,
        );
        let mut out = Out::new();
        write_information(Some(&smbios), &bios, &product, &mut out);
        let enriched = out.finish();
        assert_eq!(
            enriched.body,
            format!(
                "{}OEM String (0x000B, 1): Serial Number: OEM2410A82716\r\n",
                section.body
            )
        );
        assert_eq!(
            enriched.ids.last().map(String::as_str),
            Some("OEM2410A82716")
        );
        for count in [0, 255] {
            smbios.structures.last_mut().expect("OEM record").formatted[4] = count;
            let mut out = Out::new();
            write_information(Some(&smbios), &bios, &product, &mut out);
            assert_eq!(
                out.finish().body,
                if count == 0 {
                    &section.body
                } else {
                    &enriched.body
                }
                .as_str()
            );
        }
        smbios
            .structures
            .last_mut()
            .expect("OEM record")
            .formatted
            .truncate(4);
        let mut out = Out::new();
        write_information(Some(&smbios), &bios, &product, &mut out);
        assert_eq!(out.finish().body, section.body);
        for placeholder in [
            "Default string",
            "To Be Filled By O.E.M.",
            "Unknown",
            "000000",
            "FFFF-FFFF",
            "\r\n",
        ] {
            assert!(!useful_new_value(placeholder));
        }
    }

    #[test]
    fn wp02_bios_enrichment_failures_are_independent_and_last() {
        let smbios = fixture();
        for failed in [
            vec!["Win32_BIOS"],
            vec!["Win32_ComputerSystemProduct"],
            vec!["Win32_BIOS", "Win32_ComputerSystemProduct"],
        ] {
            let mut calls = Vec::new();
            let mut out = Out::new();
            collect_with(Some(&smbios), &mut out, |class, names| {
                calls.push(class.to_owned());
                assert_eq!(
                    names,
                    if class == "Win32_BIOS" {
                        ["SMBIOSBIOSVersion", "SerialNumber"]
                    } else {
                        ["Vendor", "IdentifyingNumber"]
                    }
                );
                if failed.contains(&class) {
                    Err(win::Error {
                        op: "WMI query",
                        code: 0x8004_1003,
                        detail: "fabricated access denied".to_owned(),
                    })
                } else {
                    Ok(vec![wmi::Row::default()])
                }
            });
            assert_eq!(calls, ["Win32_BIOS", "Win32_ComputerSystemProduct"]);
            let section = out.finish();
            assert!(section.body.contains("Version: 1.E2\r\nBIOS Version: \r\n"));
            assert!(
                section
                    .body
                    .contains("UUID: 78563A12-9ABC-DEF0-8245-6789ABCDEF10\r\n")
            );
            assert!(section.body.contains("Vendor: \r\n"));
            assert!(
                section
                    .body
                    .contains("IdentifyingNumber: \r\nSerialNumber: \r\n")
            );
            assert_eq!(section.failures.len(), failed.len());
            let mut ending = "System Version: 1.0\r\n".to_owned();
            for class in failed {
                ending.push_str(&format!(
                    "WMI query failed: {class}: WMI query failed: 0x80041003 fabricated access denied\r\n"
                ));
            }
            assert!(section.body.ends_with(&ending));
            assert!(!section.body.contains("Error retrieving"));
        }
        let mut out = Out::new();
        collect_with(None, &mut out, |class, names| {
            assert_eq!(
                names,
                if class == "Win32_BIOS" {
                    [
                        "Manufacturer",
                        "Version",
                        "SMBIOSBIOSVersion",
                        "SerialNumber",
                    ]
                    .as_slice()
                } else {
                    ["Vendor", "UUID", "IdentifyingNumber"].as_slice()
                }
            );
            Ok(Vec::new())
        });
        assert_eq!(
            out.finish().body,
            concat!(
                "Manufacturer: \r\nVendor: \r\nVersion: \r\nBIOS Version: \r\n",
                "UUID: \r\nIdentifyingNumber: \r\nSerialNumber: \r\n",
            )
        );
        let mut partial = fixture();
        for record in &mut partial.structures {
            match record.kind {
                0 => record.formatted[5] = 0,
                1 => record.formatted.truncate(8),
                _ => {}
            }
        }
        collect_with(Some(&partial), &mut Out::new(), |class, names| {
            assert_eq!(
                names,
                if class == "Win32_BIOS" {
                    ["Version", "SMBIOSBIOSVersion", "SerialNumber"]
                } else {
                    ["Vendor", "UUID", "IdentifyingNumber"]
                }
            );
            Ok(Vec::new())
        });
    }

    #[test]
    #[ignore = "Reads real identifiers; redirect stdout to the private golden/wp-02 folder."]
    fn wp02_live_capture() {
        use std::{sync::mpsc, thread, time::Duration};

        let (sender, receiver) = mpsc::channel();
        thread::spawn(move || {
            let ctx = Ctx::new();
            let sections: Vec<_> = crate::hw::PROVIDERS
                .iter()
                .filter(|provider| matches!(provider.title, "MOTHERBOARD" | "CHASSIS" | "(SM)BIOS"))
                .map(|provider| crate::hw::collect_provider(provider, &ctx))
                .collect();
            let raw = win::firmware::raw_table(0x5253_4d42, 0);
            let mut faults = Vec::new();
            for failed in ["Win32_BIOS", "Win32_ComputerSystemProduct"] {
                let mut out = Out::new();
                collect_with(ctx.smbios(), &mut out, |class, names| {
                    if class == failed {
                        Err(win::Error::msg(
                            "WMI query",
                            "injected single-query failure",
                        ))
                    } else {
                        wmi::query(
                            wmi::Namespace::Cimv2,
                            &format!("SELECT {} FROM {class}", names.join(", ")),
                        )
                    }
                });
                let section = out.finish();
                assert_eq!(section.failures.len(), 1);
                let (class, names) = if failed == "Win32_BIOS" {
                    (
                        "Win32_ComputerSystemProduct",
                        ["Vendor", "UUID", "IdentifyingNumber"].as_slice(),
                    )
                } else {
                    (
                        "Win32_BIOS",
                        ["SMBIOSBIOSVersion", "SerialNumber"].as_slice(),
                    )
                };
                let rows = wmi::query(
                    wmi::Namespace::Cimv2,
                    &format!("SELECT {} FROM {class}", names.join(", ")),
                )
                .expect("unaffected live WMI query");
                let row = rows.last().expect("unaffected live query has data");
                for name in names {
                    let value = row.str(name).unwrap_or_default();
                    let label = if *name == "SMBIOSBIOSVersion" {
                        "BIOS Version"
                    } else {
                        name
                    };
                    // A direct SMBIOS UUID takes precedence over WMI; all other fields stay WMI-only.
                    if *name != "UUID" {
                        assert!(section.body.contains(&format!("{label}: {value}\r\n")));
                    }
                }
                faults.push((failed, section));
            }
            sender
                .send((sections, raw, faults))
                .expect("capture receiver");
        });
        let (sections, raw, faults) = receiver
            .recv_timeout(Duration::from_secs(60))
            .expect("WP-02 capture completed within 60 seconds");
        let root = std::path::Path::new("D:/GIT/HWID-Privacy/app/rust/golden/wp-02");
        std::fs::create_dir_all(root).expect("private capture directory");
        for (class, section) in faults {
            std::fs::write(
                root.join(format!("minors-bios-failed-{class}.txt")),
                &section.body,
            )
            .expect("private single-query failure report");
            std::fs::write(
                root.join(format!("minors-bios-failed-{class}.diag.txt")),
                section.failures.join("\r\n"),
            )
            .expect("private single-query failure diagnostics");
        }
        let report = crate::hw::full_report(&sections);
        std::fs::write(root.join("rust-report.txt"), &report).expect("private report");
        let mut diagnostics = format!("Administrator: {}\r\n", win::security::is_admin());
        for section in &sections {
            diagnostics.push_str(&format!(
                "{}: {} ms; source: {}\r\n",
                section.title, section.elapsed_ms, section.source
            ));
            for failure in &section.failures {
                diagnostics.push_str(&format!("  {failure}\r\n"));
            }
        }
        std::fs::write(root.join("rust-report.txt.diag.txt"), &diagnostics)
            .expect("private diagnostics");
        std::fs::write(
            root.join("raw-smbios.bin"),
            raw.expect("raw firmware capture"),
        )
        .expect("private raw firmware");
        print!("{report}{diagnostics}");
    }
}
