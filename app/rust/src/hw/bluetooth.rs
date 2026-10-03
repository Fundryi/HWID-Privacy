//! Per-radio Bluetooth identities, with the legacy WMI/registry fallback chain.

use crate::{hw::Ctx, report::Out, win};
use win::wmi::{self, Namespace};

/// Collects this hardware section through the shared output builder.
pub fn collect(_ctx: &Ctx, out: &mut Out) -> Result<(), win::Error> {
    match win::bluetooth::radios() {
        Ok(scan) => {
            for error in &scan.failures {
                out.fallback_failed("native Bluetooth radio API", error);
            }
            if !scan.radios.is_empty() {
                out.source("native Bluetooth radio API");
                let adapters: Vec<_> = scan
                    .radios
                    .iter()
                    .map(|radio| (radio.name.clone(), radio.mac_address()))
                    .collect();
                append_adapters(out, &adapters);
                // AD-03: retain working radios and explain any radio that could not be read.
                for error in &scan.failures {
                    if error.op != "BluetoothFindRadioClose" {
                        out.info("Error", &error.to_string());
                    }
                }
                return Ok(());
            }
        }
        Err(error) => {
            out.fallback_failed("native Bluetooth radio API", &error);
        }
    }
    legacy(out)
}

fn legacy(out: &mut Out) -> Result<(), win::Error> {
    let mut adapters = Vec::new();
    let mut unresolved = Vec::new();
    // C# parity: Hardware/BluetoothInfo.cs:24-47. Keep this exact USB-name query.
    match wmi::query(
        Namespace::Cimv2,
        "SELECT Name, PNPDeviceID FROM Win32_PnPEntity WHERE PNPDeviceID LIKE 'USB%' AND Name LIKE '%Bluetooth%'",
    ) {
        Ok(rows) if !rows.is_empty() => {
            let mac = registry_mac(out, &mut unresolved);
            out.source(if mac.is_some() {
                "WMI USB Bluetooth + registry"
            } else {
                "WMI USB Bluetooth"
            });
            for row in rows {
                let name = row
                    .str("Name")
                    .unwrap_or_else(|| "Unknown Bluetooth Adapter".into());
                // C# parity: Hardware/BluetoothInfo.cs:35-44. Only this fallback
                // repeats the one global address; the native path pairs each radio.
                adapters.push((
                    name,
                    mac.clone().unwrap_or_else(|| "MAC not available".into()),
                ));
            }
        }
        Ok(_) => {
            // An empty successful query is the normal signal to try the next source.
        }
        Err(error) => {
            out.fallback_failed("WMI USB Bluetooth", &error);
            unresolved.push(error);
        }
    }
    // C# parity: Hardware/BluetoothInfo.cs:49-60. Registry alone comes second.
    if adapters.is_empty()
        && let Some(mac) = registry_mac(out, &mut unresolved)
    {
        out.source("registry");
        adapters.push(("Bluetooth Adapter".into(), mac));
    }
    // C# parity: Hardware/BluetoothInfo.cs:63-77. Do not read a MAC for BTHUSB rows.
    if adapters.is_empty() {
        match wmi::query(
            Namespace::Cimv2,
            "SELECT Name, PNPDeviceID FROM Win32_PnPEntity WHERE Service = 'BTHUSB'",
        ) {
            Ok(rows) => {
                out.source("WMI BTHUSB");
                for row in rows {
                    adapters.push((
                        row.str("Name")
                            .unwrap_or_else(|| "Unknown Bluetooth Adapter".into()),
                        "MAC not available".into(),
                    ));
                }
            }
            Err(error) => {
                out.fallback_failed("WMI BTHUSB", &error);
                unresolved.push(error);
            }
        }
    }
    append_adapters(out, &adapters);
    // AD-03: successful fallbacks suppress failures in the report; unresolved
    // address failures or a completely empty failed chain remain visible.
    for error in &unresolved {
        if adapters.is_empty() || error.op == "RegOpenKeyExW" || error.op == "RegGetValueW" {
            out.info("Error", &error.to_string());
        }
    }
    Ok(())
}

fn registry_mac(out: &mut Out, unresolved: &mut Vec<win::Error>) -> Option<String> {
    match win::bluetooth::legacy_registry_mac() {
        Ok(mac) => mac,
        Err(error) => {
            out.fallback_failed("registry LocalRadioAddress", &error);
            unresolved.push(error);
            None
        }
    }
}

fn append_adapters(out: &mut Out, adapters: &[(String, String)]) {
    // C# parity: Hardware/BluetoothInfo.cs:80-94. No trailing item separator.
    if adapters.is_empty() {
        out.text("No Bluetooth adapters detected.");
    }
    for (index, (name, mac)) in adapters.iter().enumerate() {
        if index != 0 {
            out.separator();
        }
        out.info("Adapter", name);
        if mac == "MAC not available" {
            out.info("MAC Address", mac);
        } else {
            out.id("MAC Address", mac);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn per_radio_text_and_no_radio_fallback() {
        let radios = [
            win::bluetooth::Radio {
                name: "Intel(R) Wireless Bluetooth(R)".into(),
                address: [0x91, 0x47, 0x2A, 0xF0, 0x8B, 0xC4],
            },
            win::bluetooth::Radio {
                name: "Generic Bluetooth Radio".into(),
                address: [0x72, 0x61, 0xB8, 0x4A, 0xE0, 0x00],
            },
        ];
        let adapters: Vec<_> = radios
            .iter()
            .map(|r| (r.name.clone(), r.mac_address()))
            .collect();
        let mut out = Out::new();
        append_adapters(&mut out, &adapters);
        let section = out.finish();
        assert_eq!(
            section.body,
            "Adapter: Intel(R) Wireless Bluetooth(R)\r\nMAC Address: C4:8B:F0:2A:47:91\r\n----------------------------------------\r\nAdapter: Generic Bluetooth Radio\r\nMAC Address: 00:E0:4A:B8:61:72\r\n"
        );
        assert_eq!(section.ids, ["C4:8B:F0:2A:47:91", "00:E0:4A:B8:61:72"]);
        let mut out = Out::new();
        append_adapters(&mut out, &[]);
        assert_eq!(out.finish().body, "No Bluetooth adapters detected.\r\n");
        let mut out = Out::new();
        append_adapters(
            &mut out,
            &[(
                "Unknown Bluetooth Adapter".into(),
                "MAC not available".into(),
            )],
        );
        let section = out.finish();
        assert_eq!(
            section.body,
            "Adapter: Unknown Bluetooth Adapter\r\nMAC Address: MAC not available\r\n"
        );
        assert!(section.ids.is_empty());
    }

    #[test]
    #[ignore = "read-only real hardware capture; identifiers stay in private golden/wp-06"]
    fn wp06_private_capture() {
        use std::{fs, path::Path, time::Instant};
        let directory = Path::new(r"D:\GIT\HWID-Privacy\app\rust\golden\wp-06");
        fs::create_dir_all(directory).expect("private capture directory");
        let mut sections = Vec::new();
        let mut diagnostics = format!("elevated: {}\r\n", win::security::is_admin());
        for (title, collect) in [
            (
                "USB DEVICES",
                crate::hw::usb::collect as fn(&Ctx, &mut Out) -> Result<(), win::Error>,
            ),
            ("BLUETOOTH ADAPTERS", super::collect),
        ] {
            let provider = crate::hw::Provider { title, collect };
            let mut samples = Vec::new();
            for index in 0..5 {
                let start = Instant::now();
                let section = crate::hw::collect_provider(&provider, &Ctx::new());
                samples.push(start.elapsed().as_micros());
                if index == 0 {
                    println!("{title}\n{}", section.body);
                    diagnostics.push_str(&format!(
                        "{title}\r\nsource: {}\r\nfailures: {:?}\r\n",
                        section.source, section.failures
                    ));
                    sections.push(section);
                }
            }
            diagnostics.push_str(&format!("samples_us: {samples:?}\r\n"));
        }
        fs::write(
            directory.join("rust-report.txt"),
            crate::hw::full_report(&sections),
        )
        .expect("private report");
        fs::write(directory.join("rust-report.diag.txt"), diagnostics)
            .expect("private diagnostics");
        fs::write(
            directory.join("radio-evidence.txt"),
            format!("{:?}\n", win::bluetooth::radios()),
        )
        .expect("private per-radio API evidence");
        fs::write(directory.join("legacy-evidence.txt"), format!("legacy registry: {:?}\nUSB-name WMI: {:?}\nBTHUSB WMI: {:?}\n", win::bluetooth::legacy_registry_mac(), wmi::query(Namespace::Cimv2, "SELECT Name, PNPDeviceID FROM Win32_PnPEntity WHERE PNPDeviceID LIKE 'USB%' AND Name LIKE '%Bluetooth%'"), wmi::query(Namespace::Cimv2, "SELECT Name, PNPDeviceID FROM Win32_PnPEntity WHERE Service = 'BTHUSB'"))).expect("private fallback evidence");
        let usb_rows = wmi::query(
            Namespace::Cimv2,
            "SELECT Name, PNPDeviceID FROM Win32_PnPEntity WHERE PNPDeviceID LIKE 'USB%'",
        )
        .expect("read-only legacy USB source cross-check");
        let usb_evidence: Vec<_> = usb_rows
            .iter()
            .map(|row| (row.str("Name"), row.str("PNPDeviceID")))
            .collect();
        fs::write(
            directory.join("usb-wmi-evidence.json"),
            serde_json::to_vec_pretty(&usb_evidence).expect("USB source evidence JSON"),
        )
        .expect("private USB source evidence");
        let mut fallback = Out::new();
        legacy(&mut fallback).expect("read-only legacy Bluetooth fallback");
        fs::write(directory.join("legacy-report.txt"), fallback.finish().body)
            .expect("private legacy fallback report");
    }
}
