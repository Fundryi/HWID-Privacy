//! Present USB devnodes with the legacy instance-tail serial filter.

use crate::{
    hw::Ctx,
    report::Out,
    win::{self, setupapi::DevInfoSet},
};
use windows::Win32::{
    Devices::DeviceAndDriverInstallation::{SPDRP_DEVICEDESC, SPDRP_DRIVER, SPDRP_FRIENDLYNAME},
    Foundation::{ERROR_INVALID_DATA, ERROR_NOT_FOUND},
};

/// Collects this hardware section through the shared output builder.
pub fn collect(_ctx: &Ctx, out: &mut Out) -> Result<(), win::Error> {
    out.source("native SetupAPI");
    let result = (|| {
        let set = DevInfoSet::enum_present_all()?;
        let device_serials = win::usbhub::serials();
        let mut first = true;
        for device in set.devices()? {
            let instance_id = match device.instance_id() {
                Ok(id) => id,
                Err(error) => {
                    // The unreadable devnode may not be USB; keep it diagnostic-only.
                    out.fallback_failed("SetupAPI instance ID", &error);
                    continue;
                }
            };
            let Some(serial) = serial_from_instance_id(&instance_id) else {
                continue;
            };
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
            append_device(out, &mut first, &name, serial);
            match device.property_string(SPDRP_DRIVER) {
                Ok(key) => {
                    if let Some(device_serial) = device_serials.get(&key.to_ascii_uppercase()) {
                        append_device_serial(out, serial, device_serial);
                    }
                }
                Err(error) => {
                    out.fallback_failed("USB driver key association", &error);
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

fn serial_from_instance_id(instance_id: &str) -> Option<&str> {
    // C# parity: Hardware/UsbInfo.cs:25-32. USBSTOR also matches; do not trim or
    // reject empty tails, all-zero tails, or a closing brace on its own.
    if !instance_id.get(..3)?.eq_ignore_ascii_case("USB") {
        return None;
    }
    let (_, serial) = instance_id.rsplit_once('\\')?;
    (!serial.contains(['&', '.', '{'])).then_some(serial)
}

fn append_device(out: &mut Out, first: &mut bool, name: &str, serial: &str) {
    // C# parity: Hardware/UsbInfo.cs:41-47. Separators occur only between devices.
    if !*first {
        out.separator();
    }
    *first = false;
    out.info("Device", name).id("Serial", serial);
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
        append_device(&mut out, &mut first, "USB Receiver", "83917A5E");
        append_device_serial(&mut out, "83917A5E", "83917A5E");
        append_device(&mut out, &mut first, "", "60A44C1F83D2");
        let section = out.finish();
        assert_eq!(
            section.body,
            "Device: USB Receiver\r\nSerial: 83917A5E\r\n----------------------------------------\r\nDevice: \r\nSerial: 60A44C1F83D2\r\n"
        );
        assert_eq!(section.ids, ["83917A5E", "60A44C1F83D2"]);
        let mut differing = Out::new();
        append_device(&mut differing, &mut true, "USB Receiver", "83917A5E");
        append_device_serial(&mut differing, "83917A5E", "83917a5e");
        let differing = differing.finish();
        assert_eq!(
            differing.body,
            "Device: USB Receiver\r\nSerial: 83917A5E\r\nSerial (device): 83917a5e\r\n"
        );
        assert_eq!(differing.ids, ["83917A5E", "83917a5e"]);
    }
}
