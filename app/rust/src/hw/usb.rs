//! Present USB devnodes with the legacy instance-tail serial filter.

use crate::{
    hw::Ctx,
    report::Out,
    win::{self, setupapi::DevInfoSet},
};
use std::collections::HashMap;
use windows::Win32::{
    Devices::DeviceAndDriverInstallation::{SPDRP_DEVICEDESC, SPDRP_DRIVER, SPDRP_FRIENDLYNAME},
    Foundation::{ERROR_INVALID_DATA, ERROR_NOT_FOUND},
};

/// Collects this hardware section through the shared output builder.
pub fn collect(_ctx: &Ctx, out: &mut Out) -> Result<(), win::Error> {
    out.source("native SetupAPI");
    let result = (|| {
        let set = DevInfoSet::enum_present_all()?;
        let mut entries = Vec::new();
        let mut key_counts = HashMap::new();
        for device in set.devices()? {
            let instance_id = match device.instance_id() {
                Ok(id) => id,
                Err(error) => {
                    // The unreadable devnode may not be USB; keep it diagnostic-only.
                    out.fallback_failed("SetupAPI instance ID", &error);
                    continue;
                }
            };
            if !instance_id
                .get(..3)
                .is_some_and(|prefix| prefix.eq_ignore_ascii_case("USB"))
            {
                continue;
            }
            let key = match device.property_string(SPDRP_DRIVER) {
                Ok(key) => {
                    let key = key.to_ascii_uppercase();
                    *key_counts.entry(key.clone()).or_insert(0_usize) += 1;
                    Some(key)
                }
                Err(error) => {
                    out.fallback_failed("USB driver key association", &error);
                    None
                }
            };
            entries.push((device, instance_id, key));
        }
        let device_strings = win::usbhub::descriptors();
        let mut first = true;
        for (device, instance_id, key) in entries {
            let serial = serial_from_instance_id(&instance_id);
            let current_key = device
                .property_string(SPDRP_DRIVER)
                .map(|key| key.to_ascii_uppercase());
            let current_instance = device.instance_id();
            let present = win::setupapi::is_present(&instance_id);
            let association = match (current_key, current_instance, present) {
                (Ok(current), Ok(id), Ok(true))
                    if key.as_deref() == Some(&current) && id == instance_id =>
                {
                    associated_strings(key.as_deref(), &key_counts, &device_strings, out)
                }
                (Err(error), _, _) | (_, Err(error), _) | (_, _, Err(error)) => {
                    out.fallback_failed("USB snapshot association", &error);
                    None
                }
                _ => {
                    out.fallback_failed(
                        "USB snapshot association",
                        &win::Error::msg(
                            "USB driver key",
                            "ambiguous: devnode changed or disappeared during hub scan",
                        ),
                    );
                    None
                }
            };
            let strings = association;
            let device_serial = strings.and_then(|strings| strings.serial.as_deref());
            let instance_tail = instance_id.rsplit_once('\\').map(|(_, tail)| tail);
            if serial.is_none()
                && !device_serial
                    .is_some_and(|value| instance_tail.is_some_and(|tail| value != tail))
            {
                continue;
            }
            let name = match device.property_string(SPDRP_FRIENDLYNAME) {
                Ok(name) => name,
                Err(error) => {
                    // Unset friendly names are normal; the description supplies the name.
                    if !matches!(error.code, code if code == ERROR_INVALID_DATA.0 || code == ERROR_NOT_FOUND.0)
                    {
                        out.fallback_failed("SetupAPI FRIENDLYNAME", &error);
                    }
                    match device.property_string(SPDRP_DEVICEDESC) {
                        Ok(name) => name,
                        Err(error)
                            if error.code == ERROR_INVALID_DATA.0
                                || error.code == ERROR_NOT_FOUND.0 =>
                        {
                            String::new()
                        }
                        Err(error) => {
                            out.fallback_failed("SetupAPI DEVICEDESC", &error);
                            error.to_string()
                        }
                    }
                }
            };
            render_device(out, &mut first, &name, &instance_id, strings);
            match set.container_id(&instance_id) {
                Ok(Some(container)) => {
                    out.id("Container ID", &container);
                }
                Ok(None) => {
                    out.fallback_failed(
                        "SetupAPI Container ID",
                        &win::Error::msg(
                            "USB Container ID",
                            "absent: present devnode/property or usable GUID unavailable",
                        ),
                    );
                }
                Err(error) => {
                    out.fallback_failed("SetupAPI Container ID", &error);
                }
            }
        }
        Ok::<_, win::Error>(())
    })();
    if let Err(error) = result {
        // C# parity: Hardware/UsbInfo.cs:49-51. Keep the fixed error and add AD-03 detail.
        out.fallback_failed("native SetupAPI", &error)
            .info("Error", "Unable to retrieve USB information")
            .info("Error", &error.to_string());
    }
    Ok(())
}

fn associated_strings<'a>(
    key: Option<&str>,
    counts: &HashMap<String, usize>,
    strings: &'a HashMap<String, win::usbhub::DeviceStrings>,
    out: &mut Out,
) -> Option<&'a win::usbhub::DeviceStrings> {
    let key = key?;
    if counts.get(key) != Some(&1) {
        out.fallback_failed(
            "USB driver key association",
            &win::Error::msg(
                "USB descriptors",
                "ambiguous: driver key shared by multiple devnodes",
            ),
        );
        return None;
    }
    match strings.get(key) {
        Some(strings) => Some(strings),
        None => {
            out.fallback_failed(
                "USB driver key association",
                &win::Error::msg(
                    "USB descriptors",
                    "absent: no exactly matched descriptor result",
                ),
            );
            None
        }
    }
}

fn render_device(
    out: &mut Out,
    first: &mut bool,
    name: &str,
    instance_id: &str,
    strings: Option<&win::usbhub::DeviceStrings>,
) -> bool {
    let serial = serial_from_instance_id(instance_id);
    let tail = instance_id
        .rsplit_once('\\')
        .map(|(_, tail)| tail)
        .unwrap_or_default();
    let device_serial = strings.and_then(|strings| strings.serial.as_deref());
    if serial.is_none() && !device_serial.is_some_and(|value| value != tail) {
        return false;
    }
    append_device(out, first, name, serial);
    if let Some(value) = device_serial {
        append_device_serial(out, tail, value);
    }
    if let Some(strings) = strings {
        if let Some(value) = &strings.manufacturer {
            out.info("Device Manufacturer", value);
        }
        if let Some(value) = &strings.product {
            out.info("Device Product", value);
        }
    }
    true
}

fn serial_from_instance_id(instance_id: &str) -> Option<&str> {
    // C# parity: Hardware/UsbInfo.cs:25-32. USBSTOR also matches; do not trim or
    // reject empty tails, all-zero tails, or a closing brace on its own.
    if !instance_id.get(..3)?.eq_ignore_ascii_case("USB") {
        return None;
    }
    let (_, serial) = instance_id.rsplit_once('\\')?;
    (!serial.contains(['&', '.', '{'])).then_some(serial)
}

fn append_device(out: &mut Out, first: &mut bool, name: &str, serial: Option<&str>) {
    // C# parity: Hardware/UsbInfo.cs:41-47. Separators occur only between devices.
    if !*first {
        out.separator();
    }
    *first = false;
    out.info("Device", name);
    if let Some(serial) = serial {
        out.id("Serial", serial);
    }
}

fn append_device_serial(out: &mut Out, serial: &str, device_serial: &str) {
    if device_serial != serial {
        out.id("Serial (device)", device_serial);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn legacy_instance_tail_filter() {
        for (id, expected) in [
            ("USB\\VID_046D&PID_C52B\\83917A5E", Some("83917A5E")),
            (
                "usbstor\\Disk&Ven_Kingston\\60A44C1F83D2",
                Some("60A44C1F83D2"),
            ),
            ("USB\\node\\000000", Some("000000")),
            ("USB\\node\\}", Some("}")),
            ("USB\\node\\", Some("")),
            ("USB\\node\\  83917A5E  ", Some("  83917A5E  ")),
            ("USB\\node\\7&123&0", None),
            ("USB\\node\\83.917", None),
            ("USB\\node\\{83917}", None),
            ("USB", None),
            ("", None),
            ("PCI\\node\\83917A5E", None),
        ] {
            assert_eq!(serial_from_instance_id(id), expected, "{id}");
        }
    }

    #[test]
    fn usb_group_text_and_identifier_records() {
        let mut out = Out::new();
        assert_eq!(out.finish().body, "");
        out = Out::new();
        let mut first = true;
        append_device(&mut out, &mut first, "USB Receiver", Some("83917A5E"));
        append_device_serial(&mut out, "83917A5E", "83917A5E");
        append_device(&mut out, &mut first, "", Some("60A44C1F83D2"));
        let section = out.finish();
        assert_eq!(
            section.body,
            "Device: USB Receiver\r\nSerial: 83917A5E\r\n----------------------------------------\r\nDevice: \r\nSerial: 60A44C1F83D2\r\n"
        );
        assert_eq!(section.ids, ["83917A5E", "60A44C1F83D2"]);
        let mut differing = Out::new();
        append_device(&mut differing, &mut true, "USB Receiver", Some("83917A5E"));
        append_device_serial(&mut differing, "83917A5E", "83917a5e");
        let differing = differing.finish();
        assert_eq!(
            differing.body,
            "Device: USB Receiver\r\nSerial: 83917A5E\r\nSerial (device): 83917a5e\r\n"
        );
        assert_eq!(differing.ids, ["83917A5E", "83917a5e"]);
    }

    #[test]
    fn usb_generated_tail_exact_join_and_legacy_failure_retention() {
        let strings = win::usbhub::DeviceStrings {
            serial: Some("R7Q291E4".into()),
            manufacturer: Some("Acme".into()),
            product: Some("USB Receiver".into()),
        };
        let map = HashMap::from([("KEY".into(), strings)]);
        let mut out = Out::new();
        let joined = associated_strings(
            Some("KEY"),
            &HashMap::from([("KEY".into(), 1)]),
            &map,
            &mut out,
        );
        assert!(render_device(
            &mut out,
            &mut true,
            "Receiver",
            "USB\\VID_046D&PID_C52B\\7&183&0",
            joined
        ));
        assert!(out.finish().body.contains("Serial (device): R7Q291E4\r\n"));
        for counts in [HashMap::new(), HashMap::from([("KEY".into(), 2)])] {
            let mut out = Out::new();
            let joined = associated_strings(Some("KEY"), &counts, &map, &mut out);
            assert!(!render_device(
                &mut out,
                &mut true,
                "Receiver",
                "USB\\node\\7&183&0",
                joined
            ));
            assert!(render_device(
                &mut out,
                &mut true,
                "Receiver",
                "USB\\node\\B8D4C721",
                joined
            ));
            let section = out.finish();
            assert_eq!(section.body, "Device: Receiver\r\nSerial: B8D4C721\r\n");
            assert_eq!(section.failures.len(), 1);
        }
        // Equal cheap-device serials are still independently joined by key.
        let mut out = Out::new();
        let joined = associated_strings(
            Some("KEY"),
            &HashMap::from([("KEY".into(), 1)]),
            &map,
            &mut out,
        );
        render_device(&mut out, &mut true, "One", "USB\\node\\7&183&0", joined);
        render_device(&mut out, &mut false, "Two", "USB\\node\\7&184&0", joined);
        assert_eq!(
            out.finish()
                .body
                .matches("Serial (device): R7Q291E4")
                .count(),
            2
        );
    }
}
