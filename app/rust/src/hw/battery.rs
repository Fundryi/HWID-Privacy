//! Battery driver identity and independent SMBIOS portable-battery corroboration.

use crate::{hw::Ctx, report::Out, win};

pub fn collect(ctx: &Ctx, out: &mut Out) -> win::Result<()> {
    out.source("Battery IOCTL + SMBIOS type 22");
    let scan = win::battery::collect();
    let firmware = ctx.smbios_result().map(win::firmware::portable_batteries);
    render_sources(&scan, &firmware, out);
    Ok(())
}

fn render_sources(
    scan: &win::Result<win::battery::Scan>,
    firmware: &win::Result<Vec<win::firmware::PortableBattery>>,
    out: &mut Out,
) {
    let mut groups = 0;
    let mut serials = Vec::new();
    match scan {
        Ok(scan) => {
            for error in &scan.failures {
                out.fallback_failed("Battery collection", error);
            }
            for (index, battery) in scan.batteries.iter().enumerate() {
                start_group(out, &mut groups);
                out.info("Battery", &format!("#{}", index + 1));
                for field in &battery.fields {
                    if field.identity
                        && let Ok(Some(value)) = &field.value
                    {
                        if value.chars().all(|character| value.starts_with(character)) {
                            out.fallback_failed(
                                field.label,
                                &win::Error::msg(
                                    "battery identity",
                                    "implausible: repeated-character identity",
                                ),
                            );
                            continue;
                        }
                        let duplicate = scan
                            .batteries
                            .iter()
                            .filter(|candidate| {
                                candidate.fields.iter().any(|other| {
                                    other.label == field.label
                                        && matches!(&other.value, Ok(Some(other)) if other == value)
                                })
                            })
                            .count()
                            > 1;
                        if duplicate {
                            out.fallback_failed(
                                field.label,
                                &win::Error::msg(
                                    "battery identity",
                                    "implausible: same identity on distinct battery interfaces",
                                ),
                            );
                            continue;
                        }
                        if field.label == "Battery Serial" {
                            serials.push(value.as_str());
                        }
                    }
                    write_field(out, field.label, field.identity, &field.value);
                }
            }
        }
        Err(error) => {
            write_field(out, "Battery Enumeration", false, &Err(error.clone()));
        }
    }
    match firmware {
        Ok(records) => {
            for (index, battery) in records.iter().enumerate() {
                start_group(out, &mut groups);
                out.info("SMBIOS Battery", &format!("#{}", index + 1));
                write_field(out, "Battery Name (SMBIOS)", false, &battery.name);
                write_field(
                    out,
                    "Battery Manufacturer (SMBIOS)",
                    false,
                    &battery.manufacturer,
                );
                write_field(
                    out,
                    "Battery Manufacture Date (SMBIOS)",
                    false,
                    &battery.date,
                );
                // There is no documented interface-to-SMBIOS join. Keep firmware
                // records separate; only omit serials already reported byte-identically.
                let serial = battery.serial.as_ref().ok().and_then(Option::as_deref);
                if serial.is_some_and(|value| {
                    records
                        .iter()
                        .filter(|other| {
                            other.serial.as_ref().ok().and_then(Option::as_deref) == Some(value)
                        })
                        .count()
                        > 1
                }) {
                    out.fallback_failed(
                        "Battery Serial (SMBIOS)",
                        &win::Error::msg(
                            "SMBIOS battery identity",
                            "implausible: same serial on distinct firmware records",
                        ),
                    );
                } else if !battery
                    .serial
                    .as_ref()
                    .ok()
                    .and_then(Option::as_deref)
                    .is_some_and(|value| serials.contains(&value))
                {
                    write_field(out, "Battery Serial (SMBIOS)", true, &battery.serial);
                } else {
                    out.fallback_failed(
                        "Battery Serial (SMBIOS)",
                        &win::Error::msg(
                            "SMBIOS battery identity",
                            "ambiguous: byte-identical IOCTL serial; no device association claimed",
                        ),
                    );
                }
            }
        }
        Err(error) => {
            write_field(out, "SMBIOS Battery", false, &Err(error.clone()));
        }
    }
    if groups == 0 && scan.is_ok() && firmware.is_ok() {
        out.text("No batteries detected.");
    }
}

fn start_group(out: &mut Out, groups: &mut usize) {
    if *groups > 0 {
        out.separator();
    }
    *groups += 1;
}

fn write_field(out: &mut Out, label: &str, identity: bool, value: &win::Result<Option<String>>) {
    match value {
        Ok(Some(value)) if identity => {
            out.id(label, value);
        }
        Ok(Some(value)) => {
            out.info(label, value);
        }
        Ok(None) => {
            out.fallback_failed(label, &win::Error::msg("battery field", "absent or empty"));
        }
        Err(error) => {
            out.fallback_failed(label, error);
            out.info(label, &format!("Unavailable ({error})"));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn battery_sources_preserve_siblings_reject_duplicates_and_do_not_join() {
        let battery = |serial: &str| win::battery::Battery {
            fields: vec![
                win::battery::Field {
                    label: "Battery Name",
                    identity: false,
                    value: Ok(Some("L19M3P71".into())),
                },
                win::battery::Field {
                    label: "Battery Serial",
                    identity: true,
                    value: Ok(Some(serial.into())),
                },
                win::battery::Field {
                    label: "Battery Manufacture Date",
                    identity: false,
                    value: Err(win::Error::msg("battery date", "implausible future date")),
                },
            ],
        };
        let firmware = || {
            Ok(vec![win::firmware::PortableBattery {
                name: Ok(Some("Firmware Battery".into())),
                manufacturer: Ok(None),
                date: Ok(None),
                serial: Ok(Some("BAT2408G7192".into())),
            }])
        };
        let mut out = Out::new();
        render_sources(
            &Ok(win::battery::Scan {
                batteries: vec![battery("BAT2408G7192")],
                failures: vec![],
            }),
            &firmware(),
            &mut out,
        );
        let single = out.finish();
        assert!(single.body.contains("Battery Serial: BAT2408G7192\r\n"));
        assert!(single.body.contains("SMBIOS Battery: #1\r\n"));
        assert!(!single.body.contains("Battery Serial (SMBIOS)"));
        assert!(
            single
                .failures
                .iter()
                .any(|failure| failure.contains("ambiguous"))
        );
        let mut out = Out::new();
        out.info("Legacy", "kept");
        render_sources(
            &Ok(win::battery::Scan {
                batteries: vec![battery("BAT2408G7192"), battery("BAT2408G7192")],
                failures: vec![],
            }),
            &firmware(),
            &mut out,
        );
        let duplicate = out.finish();
        assert!(duplicate.body.starts_with("Legacy: kept\r\n"));
        assert!(!duplicate.body.contains("Battery Serial: "));
        assert!(duplicate.body.contains("Battery Name: L19M3P71\r\n"));
        assert!(
            duplicate
                .body
                .contains("Battery Serial (SMBIOS): BAT2408G7192\r\n")
        );
        assert!(
            duplicate
                .failures
                .iter()
                .any(|failure| failure.contains("implausible"))
        );
        assert!(
            duplicate
                .failures
                .iter()
                .all(|failure| !failure.contains("BAT2408G7192"))
        );
        let mut out = Out::new();
        render_sources(&Ok(win::battery::Scan::default()), &Ok(vec![]), &mut out);
        assert_eq!(out.finish().body, "No batteries detected.\r\n");
        let mut out = Out::new();
        render_sources(
            &Err(win::Error::msg("fixture", "access-denied")),
            &firmware(),
            &mut out,
        );
        let failed = out.finish();
        assert!(failed.body.contains("Battery Enumeration: Unavailable ("));
        assert!(
            failed
                .body
                .contains("Battery Serial (SMBIOS): BAT2408G7192\r\n")
        );
        assert!(!failed.failures.is_empty());
    }

    #[test]
    fn marked_battery_values_mask_without_losing_partial_fields() {
        let mut out = Out::new();
        for (label, value) in [
            ("Battery Serial", "BAT2402A7381"),
            ("Battery Unique ID", "SMP-L18M3P73-20240229-4A37"),
            ("Battery Serial (SMBIOS)", "4A37"),
        ] {
            write_field(&mut out, label, true, &Ok(Some(value.to_owned())));
        }
        write_field(
            &mut out,
            "Battery Manufacturer",
            false,
            &Ok(Some("SMP".to_owned())),
        );
        write_field(
            &mut out,
            "Battery Manufacture Date",
            false,
            &Err(win::Error::msg("battery date", "malformed calendar date")),
        );
        let section = out.finish();
        let masked = crate::report::masked(&section);
        assert_eq!(section.ids.len(), 3);
        for value in &section.ids {
            assert!(!masked.body.contains(value));
        }
        assert!(masked.body.contains("Battery Manufacturer: SMP\r\n"));
        assert!(masked.body.contains("Battery Serial (SMBIOS): XXXX\r\n"));
        assert!(
            masked
                .body
                .contains("Battery Manufacture Date: Unavailable (battery date failed:")
        );
        assert_eq!(section.failures.len(), 1);
        assert!(!section.body.replace("\r\n", "").contains('\n'));
    }
}
