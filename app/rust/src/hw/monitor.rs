//! WMI monitor identities with exact-instance EDID enrichment and registry fallback.

use crate::{
    hw::Ctx,
    report::Out,
    win::{self, edid, registry, wmi},
};

const ENUM_ROOT: &str = r"SYSTEM\CurrentControlSet\Enum";
const NO_MONITORS: &str =
    "No monitors detected. Please ensure your display drivers are properly installed.";
type Details = Vec<(&'static str, String)>;

/// Collects this hardware section through the shared output builder.
pub fn collect(ctx: &Ctx, out: &mut Out) -> Result<(), win::Error> {
    // The shared collect_all worker supplies the 60-second WMI/provider deadline.
    // C# parity: Hardware/MonitorInfo.cs:31. Do not filter Active or reorder rows.
    match wmi::query(wmi::Namespace::Wmi, "SELECT * FROM WmiMonitorID") {
        Ok(rows) if !rows.is_empty() => {
            out.source("WMI");
            out.info("Count", &format!("{} monitor(s) found:", rows.len()))
                .blank();
            for (index, row) in rows.iter().enumerate() {
                if index != 0 {
                    out.separator();
                }
                match wmi_details(row) {
                    Ok(mut details) => {
                        let identity = row.str("InstanceName").ok_or_else(|| {
                            win::Error::msg("WmiMonitorID.InstanceName", "missing instance name")
                        });
                        let parsed = identity
                            .and_then(|name| edid::instance_id(&name))
                            .and_then(|id| read_edid(&id).map(|edid| (id, edid)));
                        match parsed {
                            Ok((id, edid)) => {
                                out.source("WMI + registry");
                                checksum_evidence(out, &edid);
                                if let Some(serial) = edid.numeric_serial {
                                    details.push(("EDID Serial (numeric)", serial.to_string()));
                                }
                                append_edid_details(&mut details, &edid, true);
                                append_override_details(out, &mut details, &id);
                            }
                            Err(error) => {
                                out.fallback_failed("registry EDID enrichment", &error);
                                // Missing optional EDID is absence, not another monitor's ID.
                                // AD-03 covers actual failures where no serial source succeeded.
                                if !is_missing(&error) {
                                    details.push(("Error", error.to_string()));
                                }
                            }
                        }
                        write_details(out, &details);
                    }
                    Err(error) => {
                        out.fallback_failed("WMI monitor details", &error);
                        // C# parity: Hardware/MonitorInfo.cs:88-90. An invalid row keeps its group.
                        out.info("Error", &format!("Error reading monitor details: {error}"));
                    }
                }
            }
        }
        result => {
            if let Err(error) = result {
                // AD-18: a thrown query now takes the same fallback as zero rows.
                out.fallback_failed("WMI", &error);
            }
            collect_registry(ctx, out);
        }
    }
    Ok(())
}

fn wmi_details(row: &wmi::Row) -> win::Result<Details> {
    let mut details = Vec::new();
    // C# parity: Hardware/MonitorInfo.cs:62-79. No trimming or numeric placeholder
    // filtering applies to WMI strings; remove every NUL element, including interior NULs.
    for (property, label) in [
        ("ManufacturerName", "Manufacturer"),
        ("UserFriendlyName", "Model"),
        ("SerialNumberID", "Serial Number"),
        ("ProductCodeID", "Product Code"),
    ] {
        let text = match row.u16_array(property) {
            Some(units) => array_text(&units),
            None if row.str(property).is_none() => String::new(),
            None => {
                return Err(win::Error::msg(
                    "WmiMonitorID property",
                    format!("{property} is not a UInt16 array"),
                ));
            }
        };
        if !text.is_empty() {
            details.push((label, text));
        }
    }
    if let (Some(week), Some(year)) = (row.str("WeekOfManufacture"), row.str("YearOfManufacture")) {
        details.push(("Manufacturing Date", format!("Week {week}, {year}")));
    }
    Ok(details)
}

fn array_text(units: &[u16]) -> String {
    let units: Vec<_> = units.iter().copied().filter(|unit| *unit != 0).collect();
    String::from_utf16_lossy(&units)
}

fn read_edid(instance_id: &str) -> win::Result<edid::Edid> {
    // F6: one exact key, never a manufacturer search or another instance on failure.
    let path = format!(r"{ENUM_ROOT}\{instance_id}\Device Parameters");
    edid::parse(&registry::read_binary(&path, "EDID")?)
}

fn collect_registry(ctx: &Ctx, out: &mut Out) {
    out.source("registry");
    let mut failures = Vec::new();
    let monitors = registry_monitors(out, &mut failures);
    if monitors.is_empty() {
        // C# parity: Hardware/MonitorInfo.cs:46. Empty success keeps the original message.
        out.text(NO_MONITORS);
    } else {
        let present = match ctx.present_instance_ids() {
            Ok(ids) => Some(ids),
            Err(error) => {
                // No absence claim from an incomplete or failed SetupAPI snapshot.
                out.fallback_failed("SetupAPI present monitor IDs", &error);
                None
            }
        };
        out.info(
            "Count",
            &format!("{} monitor(s) found (from registry):", monitors.len()),
        )
        .blank();
        for (index, (instance_id, edid)) in monitors.into_iter().enumerate() {
            if index != 0 {
                out.separator();
            }
            checksum_evidence(out, &edid);
            write_registry_monitor(out, &instance_id, &edid, present);
            let mut details = Vec::new();
            append_override_details(out, &mut details, &instance_id);
            write_details(out, &details);
        }
    }
    for error in failures {
        out.info("Error", &error.to_string());
    }
}

fn write_registry_monitor(
    out: &mut Out,
    instance_id: &str,
    edid: &edid::Edid,
    present: Option<&std::collections::HashSet<String>>,
) {
    // C# parity: Hardware/MonitorInfo.cs:199-224. Keep every legacy field in order.
    out.info("Manufacturer", &edid.manufacturer);
    write_details(out, &edid.descriptors);
    if let Some(serial) = edid.numeric_serial {
        out.id("EDID Serial (numeric)", &serial.to_string());
    }
    // AD-19: after all legacy fields, at most once, only in registry fallback.
    if present.is_some_and(|ids| !ids.contains(&instance_id.to_ascii_uppercase())) {
        out.info("Presence", "Not connected");
    }
    let mut details = Vec::new();
    append_edid_details(&mut details, edid, false);
    write_details(out, &details);
}

fn append_edid_details(details: &mut Details, edid: &edid::Edid, wmi: bool) {
    // The unqualified identity lines above remain WMI values. Never replace them
    // with a differing registry value or claim that this is a fresh EEPROM read.
    let mut fields = vec![(
        "Product Code",
        "EDID Product Code (registry)",
        format!("{:04X}", edid.product_code),
    )];
    if wmi {
        fields.push((
            "Manufacturer",
            "EDID Manufacturer (registry)",
            edid.manufacturer.clone(),
        ));
    }
    for (label, value) in &edid.descriptors {
        if *label == "Serial Number" {
            if !details
                .iter()
                .any(|(key, text)| *key == "Serial Number" && text == value)
            {
                details.push(("EDID Serial (text, registry)", value.clone()));
            }
        } else if wmi {
            fields.push(("Model", "EDID Model (registry)", value.clone()));
        }
    }
    match &edid.date {
        Some(edid::Date::Manufactured { week, year }) => {
            let value = match week {
                Some(week) => format!("Week {week}, {year}"),
                None => format!("{year} (week unspecified)"),
            };
            fields.push((
                "Manufacturing Date",
                "EDID Manufacturing Date (registry)",
                value,
            ));
        }
        Some(edid::Date::ModelYear(year)) => {
            details.push(("EDID Model Year (registry)", year.to_string()))
        }
        None => {}
    }
    for (legacy_label, label, value) in fields {
        if !wmi
            || !details
                .iter()
                .any(|(key, text)| *key == legacy_label && *text == value)
        {
            details.push((label, value));
        }
    }
}

fn append_override_details(out: &mut Out, details: &mut Details, instance_id: &str) {
    let path = format!(r"{ENUM_ROOT}\{instance_id}\Device Parameters\EDID_OVERRIDE");
    match registry::subkeys(&path) {
        Ok(_) => details.push((
            "EDID Override Key",
            "Present (registry; effective override not verified)".into(),
        )),
        Err(error) if is_missing(&error) => {} // Missing optional key is normal absence.
        Err(error) => {
            out.fallback_failed("registry EDID override key", &error);
        }
    }
}

fn registry_monitors(out: &mut Out, failures: &mut Vec<win::Error>) -> Vec<(String, edid::Edid)> {
    let mut monitors = Vec::new();
    let root = format!(r"{ENUM_ROOT}\DISPLAY");
    let models = match registry::subkeys(&root) {
        Ok(models) => models,
        Err(error) => {
            registry_failure(out, failures, error);
            return monitors;
        }
    };
    // C# parity: Hardware/MonitorInfo.cs:183-188. Registry enumeration order is retained.
    for model in models {
        let instances = match registry::subkeys(&format!(r"{root}\{model}")) {
            Ok(instances) => instances,
            Err(error) => {
                registry_failure(out, failures, error);
                continue;
            }
        };
        for instance in instances {
            let id = format!(r"DISPLAY\{model}\{instance}");
            match read_edid(&id) {
                Ok(edid) => monitors.push((id, edid)),
                Err(error) => registry_failure(out, failures, error),
            }
        }
    }
    monitors
}

fn registry_failure(out: &mut Out, failures: &mut Vec<win::Error>, error: win::Error) {
    out.fallback_failed("registry EDID", &error);
    if !is_missing(&error) {
        failures.push(error);
    }
}

fn is_missing(error: &win::Error) -> bool {
    matches!(error.code, 2 | 3) // Missing optional Device Parameters/EDID is normal absence.
}

fn checksum_evidence(out: &mut Out, edid: &edid::Edid) {
    if !edid.checksum_valid {
        out.fallback_failed(
            "EDID checksum",
            &win::Error::msg(
                "EDID checksum",
                "base-block sum is nonzero; identity data retained",
            ),
        );
    }
}

fn write_details(out: &mut Out, details: &[(&str, String)]) {
    for (label, value) in details {
        if matches!(
            *label,
            "Serial Number" | "EDID Serial (numeric)" | "EDID Serial (text, registry)"
        ) {
            out.id(label, value);
        } else {
            out.info(label, value);
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)] // Fixture assertions may panic; production code may not.
mod tests {
    use super::*;

    #[test]
    fn wp08_historical_presence_requires_a_successful_snapshot() {
        let fixture: serde_json::Value =
            serde_json::from_str(include_str!("../../tests/fixtures/wp-08/monitors.json")).unwrap();
        let historical = &fixture["registry"][2];
        let hex = historical["edid_hex"].as_str().unwrap();
        let bytes: Vec<_> = (0..hex.len())
            .step_by(2)
            .map(|offset| u8::from_str_radix(&hex[offset..offset + 2], 16).unwrap())
            .collect();
        let parsed = edid::parse(&bytes).unwrap();
        let id = historical["instance_id"].as_str().unwrap();
        let ids = fixture["present_instances"]
            .as_array()
            .unwrap()
            .iter()
            .map(|id| id.as_str().unwrap().to_ascii_uppercase())
            .collect();
        for (present, disconnected) in [(Some(&ids), true), (None, false)] {
            let mut out = Out::new();
            write_registry_monitor(&mut out, id, &parsed, present);
            let section = out.finish();
            assert_eq!(
                section.body,
                format!(
                    "Manufacturer: DEL\r\nModel: DELL U2415\r\nSerial Number: P2N7V46\r\nEDID Serial (numeric): 1837771573\r\n{}EDID Serial (text, registry): P2N7V46\r\nEDID Product Code (registry): 4321\r\nEDID Manufacturing Date (registry): Week 7, 2015\r\n",
                    if disconnected {
                        "Presence: Not connected\r\n"
                    } else {
                        ""
                    }
                )
            );
            assert_eq!(section.ids, ["P2N7V46", "1837771573", "P2N7V46"]);
        }
        let mut out = Out::new();
        write_registry_monitor(
            &mut out,
            fixture["present_instances"][0].as_str().unwrap(),
            &parsed,
            Some(&ids),
        );
        assert!(!out.finish().body.contains("Presence:"));
        assert_eq!(array_text(&[32, 65, 0, 66, 0, 32]), " AB ");
        assert_eq!(array_text(&[0xd83d, 0, 0xde00]), "😀");

        // Source disagreement must keep WMI bytes, including whitespace, intact.
        // The same parser fixture supplies independently specified registry values.
        let legacy = vec![
            ("Manufacturer", "DEL".into()),
            ("Model", "WMI model ".into()),
            ("Serial Number", "WMI-SERIAL".into()),
            ("Product Code", "ABCD".into()),
            ("Manufacturing Date", "Week 8, 2016".into()),
            ("EDID Serial (numeric)", "1837771573".into()),
        ];
        let mut details = legacy.clone();
        append_edid_details(&mut details, &parsed, true);
        assert_eq!(&details[..legacy.len()], &legacy);
        assert!(!details.iter().any(|(label, _)| *label == "EDID Source"));
        for expected in [
            ("EDID Serial (text, registry)", "P2N7V46".into()),
            ("EDID Product Code (registry)", "4321".into()),
            ("EDID Model (registry)", "DELL U2415".into()),
            ("EDID Manufacturing Date (registry)", "Week 7, 2015".into()),
        ] {
            assert!(details.contains(&expected));
        }
        assert!(
            !details
                .iter()
                .any(|(label, _)| *label == "EDID Manufacturer (registry)")
        );
        let mut matching = vec![
            ("Manufacturer", "DEL".into()),
            ("Model", "DELL U2415".into()),
            ("Serial Number", "P2N7V46".into()),
            ("Product Code", "4321".into()),
            ("Manufacturing Date", "Week 7, 2015".into()),
        ];
        let unchanged = matching.clone();
        append_edid_details(&mut matching, &parsed, true);
        assert_eq!(matching, unchanged);
        // Byte differences, including whitespace, still carry information.
        matching[2].1.push(' ');
        append_edid_details(&mut matching, &parsed, true);
        assert_eq!(matching.len(), unchanged.len() + 1);
        assert_eq!(
            matching.last().unwrap(),
            &("EDID Serial (text, registry)", "P2N7V46".into())
        );
        let mut missing = Vec::new();
        append_edid_details(&mut missing, &parsed, true);
        assert!(missing.contains(&("EDID Serial (text, registry)", "P2N7V46".into())));
        assert!(missing.contains(&("EDID Manufacturer (registry)", "DEL".into())));
        assert!(missing.contains(&("EDID Product Code (registry)", "4321".into())));
    }

    #[test]
    #[ignore = "read-only owner-PC capture; real identifiers remain in the private golden folder"]
    fn wp08_capture_monitor() {
        use crate::hw::{Provider, collect_provider};
        use std::{fs, sync::mpsc, thread, time::Duration};

        let (sender, receiver) = mpsc::channel();
        thread::spawn(move || {
            let ctx = Ctx::new();
            let section = collect_provider(
                &Provider {
                    title: "MONITOR INFORMATION",
                    collect,
                },
                &ctx,
            );
            let registry_section = collect_provider(
                &Provider {
                    title: "MONITOR INFORMATION",
                    collect: |ctx, out| {
                        collect_registry(ctx, out);
                        Ok(())
                    },
                },
                &ctx,
            );
            let mut records = Vec::new();
            let root = format!(r"{ENUM_ROOT}\DISPLAY");
            for model in registry::subkeys(&root).expect("DISPLAY keys") {
                for instance in
                    registry::subkeys(&format!(r"{root}\{model}")).expect("instance keys")
                {
                    let id = format!(r"DISPLAY\{model}\{instance}");
                    let path = format!(r"{ENUM_ROOT}\{id}\Device Parameters");
                    match registry::read_binary(&path, "EDID") {
                        Ok(bytes) => {
                            records.push(serde_json::json!({"instance_id": id, "bytes": bytes}))
                        }
                        Err(error) => records.push(
                            serde_json::json!({"instance_id": id, "error": error.to_string()}),
                        ),
                    }
                }
            }
            let mapping: Vec<_> = wmi::query(wmi::Namespace::Wmi, "SELECT * FROM WmiMonitorID")
                .expect("WmiMonitorID capture")
                .iter()
                .map(|row| {
                    serde_json::json!({
                        "instance_name": row.str("InstanceName"),
                        "manufacturer": row.u16_array("ManufacturerName"),
                        "model": row.u16_array("UserFriendlyName"),
                        "serial": row.u16_array("SerialNumberID"),
                        "product": row.u16_array("ProductCodeID"),
                        "week": row.str("WeekOfManufacture"), "year": row.str("YearOfManufacture")
                    })
                })
                .collect();
            sender
                .send((section, registry_section, records, mapping))
                .expect("capture receiver");
        });
        let (section, registry_section, records, mapping) = receiver
            .recv_timeout(Duration::from_secs(60))
            .expect("capture deadline");
        // Keep parallel phase captures out of the original WP-08 evidence folder.
        let folder = std::env::var_os("HWID_MONITOR_CAPTURE_DIR")
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|| r"D:\GIT\HWID-Privacy\app\rust\golden\wp-08".into());
        fs::create_dir_all(&folder).expect("private golden directory");
        fs::write(folder.join("rust-monitor.txt"), &section.body).expect("private report");
        fs::write(
            folder.join("rust-registry-monitor.txt"),
            &registry_section.body,
        )
        .expect("private registry report");
        fs::write(
            folder.join("capture.json"),
            serde_json::to_vec_pretty(&serde_json::json!({
                "registry": records, "wmi": mapping, "elapsed_ms": section.elapsed_ms,
                    "source": section.source, "failures": section.failures
                    , "registry_elapsed_ms": registry_section.elapsed_ms,
                    "registry_failures": registry_section.failures
            }))
            .expect("capture JSON"),
        )
        .expect("private capture");
        println!("{}", section.body);
        println!(
            "Elapsed: {} ms; source: {}; failures: {:?}",
            section.elapsed_ms, section.source, section.failures
        );
    }
}
