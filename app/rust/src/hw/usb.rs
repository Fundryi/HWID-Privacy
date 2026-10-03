//! Present USB devnodes with the legacy instance-tail serial filter.

use crate::{
    hw::{Ctx, first_ok},
    report::Out,
    win::{self, setupapi::DevInfoSet},
};
use windows::Win32::Devices::DeviceAndDriverInstallation::{SPDRP_DEVICEDESC, SPDRP_FRIENDLYNAME};

/// Collects this hardware section through the shared output builder.
pub fn collect(_ctx: &Ctx, out: &mut Out) -> Result<(), win::Error> {
    out.source("native SetupAPI");
    let result = (|| {
        let set = DevInfoSet::enum_present_all()?;
        let mut first = true;
        for device in set.devices()? {
            let instance_id = match device.instance_id() {
                Ok(id) => id,
                Err(error) => {
                    out.fallback_failed("SetupAPI instance ID", &error)
                        .info("Error", &error.to_string());
                    continue;
                }
            };
            let Some(serial) = serial_from_instance_id(&instance_id) else {
                continue;
            };
            let friendly = || device.property_string(SPDRP_FRIENDLYNAME);
            let description = || device.property_string(SPDRP_DEVICEDESC);
            let name = match first_ok(
                out,
                "USB name",
                &[
                    ("SetupAPI FRIENDLYNAME", &friendly),
                    ("SetupAPI DEVICEDESC", &description),
                ],
            ) {
                Ok(name) => name,
                Err(error) => error.to_string(),
            };
            append_device(out, &mut first, &name, serial);
        }
        Ok::<_, win::Error>(())
    })();
    if let Err(error) = result {
        // C# parity: Hardware/UsbInfo.cs:49-51. Keep the fixed error and add AD-03 detail.
        out.fallback_failed("native SetupAPI", &error)
            .info("Error", "Unable to retrieve USB information")
            .info("Error", &error.to_string());
    }
    out.source("native SetupAPI");
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
        append_device(&mut out, &mut first, "", "60A44C1F83D2");
        let section = out.finish();
        assert_eq!(
            section.body,
            "Device: USB Receiver\r\nSerial: 83917A5E\r\n----------------------------------------\r\nDevice: \r\nSerial: 60A44C1F83D2\r\n"
        );
        assert_eq!(section.ids, ["83917A5E", "60A44C1F83D2"]);
    }
}
