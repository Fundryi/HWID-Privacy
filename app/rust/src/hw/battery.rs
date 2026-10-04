//! Battery driver identity and independent SMBIOS portable-battery corroboration.

use crate::{hw::Ctx, report::Out, win};

pub fn collect(ctx: &Ctx, out: &mut Out) -> win::Result<()> {
    out.source("Battery IOCTL + SMBIOS type 22");
    let scan = win::battery::collect();
    let firmware = ctx.smbios_result().map(win::firmware::portable_batteries);
    let mut groups = 0;
    let mut serials = Vec::new();
    match &scan {
        Ok(scan) => {
            for error in &scan.failures {
                out.fallback_failed("Battery tag retry", error);
            }
            for (index, battery) in scan.batteries.iter().enumerate() {
                start_group(out, &mut groups);
                out.info("Battery", &format!("#{}", index + 1));
                for field in &battery.fields {
                    if field.label == "Battery Serial"
                        && let Ok(Some(value)) = &field.value
                    {
                        serials.push(value.as_str());
                    }
                    write_field(out, field.label, field.identity, &field.value);
                }
            }
        }
        Err(error) => {
            write_field(out, "Battery Enumeration", false, &Err(error.clone()));
        }
    }
    match &firmware {
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
                if !battery
                    .serial
                    .as_ref()
                    .ok()
                    .and_then(Option::as_deref)
                    .is_some_and(|value| serials.contains(&value))
                {
                    write_field(out, "Battery Serial (SMBIOS)", true, &battery.serial);
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
    Ok(())
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
