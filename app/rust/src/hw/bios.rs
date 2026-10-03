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
    let has_information = collect_with(smbios, out, |class| {
        wmi::query(wmi::Namespace::Cimv2, &format!("SELECT * FROM {class}"))
    });
    // AD-03: show the firmware failure when WMI supplies no replacement values.
    if !has_information && let Some(error) = firmware_error {
        return Err(error);
    }
    Ok(())
}

fn collect_with(
    smbios: Option<&Smbios>,
    out: &mut Out,
    mut query: impl FnMut(&str) -> win::Result<Vec<wmi::Row>>,
) -> bool {
    let mut failures = Vec::new();
    let bios = query_fields(
        "Win32_BIOS",
        [
            "Manufacturer",
            "Version",
            "SMBIOSBIOSVersion",
            "SerialNumber",
        ],
        &mut query,
        &mut failures,
    );
    let product = query_fields(
        "Win32_ComputerSystemProduct",
        ["Vendor", "UUID", "IdentifyingNumber"],
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
    query: &mut impl FnMut(&str) -> win::Result<Vec<wmi::Row>>,
    failures: &mut Vec<(&'static str, win::Error)>,
) -> [String; N] {
    match query(class) {
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

fn write_information(
    smbios: Option<&Smbios>,
    bios: &[String; 4],
    product: &[String; 3],
    out: &mut Out,
) {
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
    let prefer_direct = |direct: &str, fallback: &str| {
        if direct.is_empty() {
            fallback.to_owned()
        } else {
            direct.to_owned()
        }
    };
    // C# parity: BiosInfo.cs:50-78. No trimming, no SystemVersion; four fields remain WMI-only.
    out.info("Manufacturer", &prefer_direct(fields[0], &bios[0]))
        .info("Vendor", &product[0])
        .info("Version", &prefer_direct(fields[1], &bios[1]))
        .info("SMBIOS Version", &bios[2]);
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
}

#[cfg(test)]
mod tests {
    use super::*;

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
                "Vendor: WMI Vendor\r\nVersion: 1.E2\r\nSMBIOS Version: 1.E1\r\n",
                "Release Date: 08/14/2025\r\nUUID: 78563A12-9ABC-DEF0-8245-6789ABCDEF10\r\n",
                "IdentifyingNumber: PRD2410A7216\r\nSerialNumber: BIOS2410G0936\r\n",
                "System Manufacturer: Micro-Star International Co., Ltd.\r\n",
                "System Product: MS-7D75\r\nSystem Serial: SYS2410D5A7286\r\n",
                "System SKU: SKU-B650-042\r\nSystem Family: Desktop Family\r\n",
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
            assert!(out.finish().body.contains(&format!("UUID: {expected}\r\n")));
        }
        repeated.formatted.truncate(8);
        repeated.formatted[7] = 0;
        smbios.structures.push(repeated);
        let mut out = Out::new();
        write_information(Some(&smbios), &bios, &product, &mut out);
        let body = out.finish().body;
        assert!(body.contains("UUID: FFFFFFFF-FFFF-FFFF-FFFF-FFFFFFFFFFFF\r\n"));
        assert!(!body.contains("System Serial:"));
        assert!(body.contains("System SKU: SKU-B650-042\r\n"));
        let mut out = Out::new();
        write_information(None, &bios, &product, &mut out);
        assert_eq!(
            out.finish().body,
            concat!(
                "Manufacturer: WMI Manufacturer\r\nVendor: WMI Vendor\r\nVersion: WMI Version\r\n",
                "SMBIOS Version: 1.E1\r\nUUID: 78563A12-9ABC-DEF0-8245-6789ABCDEF11\r\n",
                "IdentifyingNumber: PRD2410A7216\r\nSerialNumber: BIOS2410G0936\r\n",
            )
        );
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
            collect_with(Some(&smbios), &mut out, |class| {
                calls.push(class.to_owned());
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
            assert!(
                section
                    .body
                    .contains("Version: 1.E2\r\nSMBIOS Version: \r\n")
            );
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
            let mut ending = "System Family: Desktop Family\r\n".to_owned();
            for class in failed {
                ending.push_str(&format!(
                    "WMI query failed: {class}: WMI query failed: 0x80041003 fabricated access denied\r\n"
                ));
            }
            assert!(section.body.ends_with(&ending));
            assert!(!section.body.contains("Error retrieving"));
        }
        let mut out = Out::new();
        collect_with(None, &mut out, |_| Ok(Vec::new()));
        assert_eq!(
            out.finish().body,
            concat!(
                "Manufacturer: \r\nVendor: \r\nVersion: \r\nSMBIOS Version: \r\n",
                "UUID: \r\nIdentifyingNumber: \r\nSerialNumber: \r\n",
            )
        );
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
                collect_with(ctx.smbios(), &mut out, |class| {
                    if class == failed {
                        Err(win::Error::msg(
                            "WMI query",
                            "injected single-query failure",
                        ))
                    } else {
                        wmi::query(wmi::Namespace::Cimv2, &format!("SELECT * FROM {class}"))
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
                let rows = wmi::query(wmi::Namespace::Cimv2, &format!("SELECT * FROM {class}"))
                    .expect("unaffected live WMI query");
                let row = rows.last().expect("unaffected live query has data");
                for name in names {
                    let value = row.str(name).unwrap_or_default();
                    let label = if *name == "SMBIOSBIOSVersion" {
                        "SMBIOS Version"
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
