//! Windows licensing and installation identifiers, in the legacy SystemInfo order.

use crate::{
    hw::{Ctx, first_ok},
    report::{Out, trim_net},
    win::{self, firmware, registry, time, wmi},
};

const CURRENT_VERSION: &str = r"SOFTWARE\Microsoft\Windows NT\CurrentVersion";
const MACHINE_GUID: &str = r"SOFTWARE\Microsoft\Cryptography";
// C# parity: Hardware/SystemInfo.cs:64-70. Keep profile 0001, not the current profile.
const HARDWARE_PROFILE: &str =
    r"SYSTEM\CurrentControlSet\Control\IDConfigDB\Hardware Profiles\0001";
const ACPI: u32 = u32::from_be_bytes(*b"ACPI");
// GetSystemFirmwareTable uses little endian for table IDs, unlike provider signatures.
const MSDM: u32 = u32::from_le_bytes(*b"MSDM");
const NATIVE_KEY: &str = "native (ACPI MSDM)";
const WMI_KEY: &str = "WMI (SoftwareLicensingService)";
const REGISTRY_ID: &str = "registry (ProductId)";
const WMI_ID: &str = "WMI (Win32_OperatingSystem)";

/// Collects system identifiers with native/registry sources and the legacy WMI fallbacks.
pub fn collect(_ctx: &Ctx, out: &mut Out) -> Result<(), win::Error> {
    let mut sources = Vec::new();
    match product_keys(out) {
        Ok((keys, source)) => {
            sources.push(source);
            for key in keys {
                write_product_key(out, key.as_deref());
            }
        }
        Err(error) => unavailable(out, "Windows Product Key", &error),
    }

    let registry_id = || {
        let value = registry::read_string(CURRENT_VERSION, "ProductId")?;
        if value.is_empty() {
            return Err(win::Error::msg("ProductId", "registry value is empty"));
        }
        Ok((vec![value], REGISTRY_ID))
    };
    let wmi_id = || wmi_product_ids().map(|values| (values, WMI_ID));
    match first_ok(
        out,
        "Product ID",
        &[(REGISTRY_ID, &registry_id), (WMI_ID, &wmi_id)],
    ) {
        Ok((values, source)) => {
            sources.push(source);
            for value in values {
                out.id("Product ID", &value);
            }
        }
        Err(error) => unavailable(out, "Product ID", &error),
    }

    // C# parity: Hardware/SystemInfo.cs:49-70. Empty GUIDs are omitted; never trim them.
    for (path, name, label, source) in [
        (
            MACHINE_GUID,
            "MachineGuid",
            "Machine GUID",
            "registry (MachineGuid)",
        ),
        (
            HARDWARE_PROFILE,
            "HwProfileGuid",
            "Hardware Profile GUID",
            "registry (HwProfileGuid)",
        ),
    ] {
        match registry::read_string(path, name) {
            Ok(value) => {
                sources.push(source);
                if !value.is_empty() {
                    out.id(label, &value);
                }
            }
            Err(error) => {
                out.fallback_failed(&format!("registry ({name})"), &error);
                unavailable(out, label, &error);
            }
        }
    }

    let dword = || {
        // C# parity: Hardware/SystemInfo.cs:73-77. Registry DWORDs stringify as signed Int32.
        registry::read_dword(CURRENT_VERSION, "InstallDate").map(|value| i64::from(value as i32))
    };
    let qword = || {
        // C# parity: Hardware/SystemInfo.cs:73-77. Registry QWORDs stringify as signed Int64.
        registry::read_qword(CURRENT_VERSION, "InstallDate").map(|value| value as i64)
    };
    let string = || {
        let value = registry::read_string(CURRENT_VERSION, "InstallDate")?;
        trim_net(&value).parse::<i64>().map_err(|_| {
            win::Error::msg("InstallDate", "registry value is not signed Unix seconds")
        })
    };
    let date = first_ok(
        out,
        "InstallDate",
        &[
            ("registry (InstallDate DWORD)", &dword),
            ("registry (InstallDate QWORD)", &qword),
            ("registry (InstallDate string)", &string),
        ],
    )
    .and_then(|seconds| {
        time::unix_to_local(seconds).ok_or_else(|| {
            let error = win::Error::msg(
                "unix_to_local",
                "InstallDate could not be converted to local time",
            );
            out.fallback_failed("Install Date", &error);
            error
        })
    });
    match date {
        Ok(value) => {
            sources.push("registry (InstallDate)");
            out.info("Install Date", &value);
        }
        Err(error) => {
            unavailable(out, "Install Date", &error);
        }
    }
    out.source(&sources.join("; "));
    Ok(())
}

fn product_keys(out: &mut Out) -> win::Result<(Vec<Option<String>>, &'static str)> {
    // PLAN WP-03: the slow licensing WMI query runs only when MSDM gives no key
    // (absent, unreadable, or malformed), so C# still gets its WMI value (AD-03).
    let native = || {
        firmware::raw_table(ACPI, MSDM)
            .and_then(|raw| parse_msdm(&raw))
            .map(|key| (vec![Some(key)], NATIVE_KEY))
    };
    let wmi = || wmi_product_keys().map(|keys| (keys, WMI_KEY));
    first_ok(
        out,
        "Windows Product Key",
        &[(NATIVE_KEY, &native), (WMI_KEY, &wmi)],
    )
}

fn wmi_product_keys() -> win::Result<Vec<Option<String>>> {
    // C# parity: Hardware/SystemInfo.cs:25-37. Preserve WMI row order and empty keys.
    let rows = wmi::query(
        wmi::Namespace::Cimv2,
        "SELECT OA3xOriginalProductKey FROM SoftwareLicensingService",
    )?;
    if rows.is_empty() {
        return Err(win::Error::msg(
            "SoftwareLicensingService",
            "WMI returned no rows",
        ));
    }
    Ok(rows
        .iter()
        .map(|row| row.str("OA3xOriginalProductKey"))
        .collect())
}

fn wmi_product_ids() -> win::Result<Vec<String>> {
    let rows = wmi::query(
        wmi::Namespace::Cimv2,
        "SELECT SerialNumber FROM Win32_OperatingSystem",
    )?;
    if rows.is_empty() {
        return Err(win::Error::msg(
            "Win32_OperatingSystem",
            "WMI returned no rows",
        ));
    }
    rows.iter()
        .map(|row| {
            row.str("SerialNumber")
                .filter(|value| !value.is_empty())
                .ok_or_else(|| win::Error::msg("Win32_OperatingSystem", "SerialNumber is empty"))
        })
        .collect()
}

fn write_product_key(out: &mut Out, key: Option<&str>) {
    // OA3 reports firmware key presence, not the installed Windows activation state.
    if let Some(key) = key.filter(|key| !key.is_empty()) {
        out.info("OEM Key in Firmware", "Yes");
        out.id("Windows Product Key", key);
    } else {
        out.info("OEM Key in Firmware", "No");
    }
}

fn unavailable(out: &mut Out, label: &str, error: &win::Error) {
    // AD-03: visible only when this item has no successful source; partial data survives.
    out.info(label, &format!("Unavailable ({error})"));
}

fn parse_msdm(raw: &[u8]) -> win::Result<String> {
    let invalid = || win::Error::msg("ACPI MSDM", "invalid or truncated product-key table");
    let header = raw.get(..56).ok_or_else(invalid)?;
    let length = u32::from_le_bytes([header[4], header[5], header[6], header[7]]) as usize;
    let data_type = u32::from_le_bytes([header[44], header[45], header[46], header[47]]);
    let data_length = u32::from_le_bytes([header[52], header[53], header[54], header[55]]);
    if &header[..4] != b"MSDM" || length != raw.len() || data_type != 1 || data_length != 29 {
        return Err(invalid());
    }
    let key = raw.get(56..85).ok_or_else(invalid)?;
    if raw.iter().fold(0_u8, |sum, byte| sum.wrapping_add(*byte)) != 0
        || !key.iter().all(|byte| byte.is_ascii_graphic())
    {
        return Err(invalid());
    }
    String::from_utf8(key.to_vec()).map_err(|_| invalid())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{fs, path::Path, sync::mpsc, thread, time::Duration};

    fn msdm_fixture() -> Vec<u8> {
        include_str!("../../tests/fixtures/wp-03/msdm.hex")
            .split_whitespace()
            .map(|byte| u8::from_str_radix(byte, 16).expect("valid fabricated MSDM byte"))
            .collect()
    }

    #[test]
    fn msdm_bounds_checksum_and_ascii_are_checked() {
        let raw = msdm_fixture();
        assert_eq!(
            parse_msdm(&raw).expect("valid MSDM"),
            "7KQ2D-9MX4W-V6R8T-P3Y5H-BNFCJ"
        );
        for length in 0..raw.len() {
            assert!(parse_msdm(&raw[..length]).is_err(), "length {length}");
        }
        for offset in [0, 4, 9, 44, 52, 56] {
            let mut malformed = raw.clone();
            malformed[offset] ^= 0x80;
            assert!(parse_msdm(&malformed).is_err(), "offset {offset}");
        }
        for byte in [0, b'\r', b'\n', 0x80, 0xff] {
            let mut malformed = raw.clone();
            malformed[56] = byte;
            malformed[9] = 0;
            malformed[9] = 0_u8.wrapping_sub(
                malformed
                    .iter()
                    .fold(0_u8, |sum, byte| sum.wrapping_add(*byte)),
            );
            assert!(parse_msdm(&malformed).is_err(), "key byte {byte}");
        }
    }

    #[test]
    fn product_key_text_reports_firmware_presence_and_untrimmed_ids() {
        let mut out = Out::new();
        for key in [None, Some(""), Some(" 7KQ2D-9MX4W-V6R8T-P3Y5H-BNFCJ ")] {
            write_product_key(&mut out, key);
        }
        let section = out.finish();
        assert_eq!(
            section.body,
            "OEM Key in Firmware: No\r\n\
             OEM Key in Firmware: No\r\n\
             OEM Key in Firmware: Yes\r\n\
             Windows Product Key:  7KQ2D-9MX4W-V6R8T-P3Y5H-BNFCJ \r\n"
        );
        assert_eq!(section.ids, [" 7KQ2D-9MX4W-V6R8T-P3Y5H-BNFCJ "]);
    }

    #[test]
    #[ignore = "reads live identifiers; output belongs only in the private golden/wp-03 folder"]
    fn wp03_capture_system() {
        if win::security::is_admin() {
            return;
        }
        let root = Path::new(r"D:\GIT\HWID-Privacy\app\rust\golden\wp-03");
        fs::create_dir_all(root).expect("private capture directory");
        let sections = crate::hw::collect_all(Some("SYSTEM INFORMATION"), &|_, _| {});
        let section = sections.first().expect("system provider exists");
        fs::write(root.join("rust-system.txt"), &section.body).expect("private report");
        fs::write(
            root.join("rust-system.diag.txt"),
            format!(
                "ElapsedMs: {}\r\nSource: {}\r\n{}\r\n",
                section.elapsed_ms,
                section.source,
                section.failures.join("\r\n")
            ),
        )
        .expect("private diagnostics");
        print!("{}", section.body);
        println!("ElapsedMs: {}", section.elapsed_ms);

        // Reference only the two changed sources against the exact C# WQL, with a deadline.
        fn reference(_ctx: &Ctx, out: &mut Out) -> win::Result<()> {
            for key in wmi_product_keys()? {
                write_product_key(out, key.as_deref());
            }
            for id in wmi_product_ids()? {
                out.id("Serial Number (Product ID)", &id);
            }
            Ok(())
        }
        let (sender, receiver) = mpsc::channel();
        thread::spawn(move || {
            let reference = crate::hw::collect_provider(
                &crate::hw::Provider {
                    title: "SYSTEM INFORMATION",
                    collect: reference,
                },
                &Ctx::new(),
            );
            sender
                .send(reference)
                .expect("reference receiver remains alive until its deadline");
        });
        let reference = receiver
            .recv_timeout(Duration::from_secs(60))
            .expect("reference deadline");
        fs::write(root.join("csharp-wql-reference.txt"), &reference.body)
            .expect("private WMI reference");
        let mut comparison = format!(
            "# WP-03 source comparison (private)\n\n\
             These historical reference values use the original C# WQL in a Rust harness.\n\
             No retired executable or comparison harness is required.\n\n\
             Changed-source prefix byte-identical: {}\n\n\
             | Old C# WQL line | New Rust line | Source | Reason / approval |\n\
             |---|---|---|---|\n",
            section.body.starts_with(&reference.body)
        );
        for old in reference.body.lines() {
            let (label, _) = old.split_once(": ").expect("reference labeled line");
            let prefix = format!("{label}: ");
            let new = section.body.lines().find(|line| line.starts_with(&prefix));
            let source = if label == "Serial Number (Product ID)" {
                "ProductId registry, WMI fallback"
            } else {
                "ACPI MSDM, WMI only when absent"
            };
            comparison.push_str(&format!(
                "| {old} | {} | {source} | {} |\n",
                new.unwrap_or("<missing>"),
                if new == Some(old) {
                    "Identical; no output-difference row"
                } else {
                    "Unapproved; owner review required"
                }
            ));
        }
        fs::write(root.join("source-comparison.md"), comparison).expect("private value comparison");
        fs::write(
            root.join("csharp-wql-reference.diag.txt"),
            format!(
                "ElapsedMs: {}\r\n{}\r\n",
                reference.elapsed_ms,
                reference.failures.join("\r\n")
            ),
        )
        .expect("private WMI diagnostics");
        match firmware::raw_table(ACPI, MSDM) {
            Ok(raw) => fs::write(root.join("msdm.bin"), raw).expect("private raw firmware table"),
            Err(error) => fs::write(root.join("msdm-unavailable.txt"), error.to_string())
                .expect("private firmware error"),
        }
    }
}
