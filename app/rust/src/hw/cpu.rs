//! Direct processor fields with the legacy WMI fallback and native CPUID identifiers.

use crate::{
    hw::Ctx,
    report::Out,
    win::{self, firmware, registry, wmi},
};
use core::arch::x86_64::{__cpuid, CpuidResult};

/// Collects this hardware section through the shared output builder.
pub fn collect(ctx: &Ctx, out: &mut Out) -> Result<(), win::Error> {
    let result = match direct_processor(ctx) {
        Ok((name, processor_id, serial)) => {
            out.source("registry, SMBIOS type 4, native (CPUID)");
            write_processor(out, &name, &processor_id, Some(&serial));
            Ok(())
        }
        Err(error) => {
            out.fallback_failed("registry / SMBIOS type 4", &error);
            let result = wmi::query(wmi::Namespace::Cimv2, "SELECT * FROM Win32_Processor");
            out.source(if result.is_ok() {
                "WMI, native (CPUID)"
            } else {
                "native (CPUID)"
            });
            if let Ok(rows) = &result {
                // C# parity: Hardware/CpuInfo.cs:23-30. No separators between sockets;
                // null serials are omitted, but empty strings and OEM placeholders stay.
                for row in rows {
                    write_processor(
                        out,
                        &row.str("Name").unwrap_or_default(),
                        &row.str("ProcessorId").unwrap_or_default(),
                        row.str("SerialNumber").as_deref(),
                    );
                }
            }
            result.map(|_| ())
        }
    };
    // x86-64 guarantees CPUID. The pinned Rust 1.98 intrinsics are safe, so the
    // provider needs neither unsafe nor a second Win32 helper implementation.
    let leaf0 = __cpuid(0);
    // C# parity: Hardware/CpuInfo.cs:38,47,62. maxLeaf is a signed Int32.
    let leaf1 = (leaf0.eax as i32 >= 1).then(|| __cpuid(1));
    let leaf3 = (leaf0.eax as i32 >= 3).then(|| __cpuid(3));
    write_cpuid(out, leaf0, leaf1, leaf3);
    // Keep the legacy failure line before the appended socket metadata. It is
    // recorded here because returning the error would render it after the new lines.
    if let Err(error) = result {
        out.fallback_failed("CPU", &error)
            .text(&format!("Error retrieving CPU information: {error}"));
    }
    if let Some(smbios) = ctx.smbios() {
        append_sockets(smbios, out);
    }
    Ok(())
}

fn append_sockets(smbios: &firmware::Smbios, out: &mut Out) {
    let mut sockets = firmware::processor_metadata(smbios);
    sockets.retain(|socket| {
        super::bios::metadata_record_has_values(
            out,
            "CPU Socket",
            Some(socket.handle),
            &[
                &socket.socket,
                &socket.manufacturer,
                &socket.part,
                &socket.asset,
            ],
        )
    });
    for (index, socket) in sockets.iter().enumerate() {
        if sockets.len() >= 2 {
            out.text(&format!("CPU Socket #{} (SMBIOS)", index + 1));
        }
        for (label, identity, value) in [
            ("Socket Designation (SMBIOS)", false, &socket.socket),
            ("Socket Manufacturer (SMBIOS)", false, &socket.manufacturer),
            ("Socket Part Number (SMBIOS)", false, &socket.part),
            ("Socket Asset Tag (SMBIOS)", true, &socket.asset),
        ] {
            // Type-4 part numbers describe the processor model; asset tags are per-unit.
            if identity {
                let values: Vec<_> = sockets.iter().map(|other| &other.asset).collect();
                super::bios::write_unique_metadata_field(out, label, value, &values);
            } else {
                super::bios::write_metadata_field(out, label, false, value);
            }
        }
    }
}

fn direct_processor(ctx: &Ctx) -> win::Result<(String, String, String)> {
    // Same OnceLock snapshot as ctx.smbios(), with the read error retained.
    let (processor_id, serial) = firmware_processor(ctx.smbios_result()?)?;
    const PROCESSORS: &str = r"HARDWARE\DESCRIPTION\System\CentralProcessor";
    let keys = registry::subkeys(PROCESSORS)?;
    if keys.is_empty() {
        return Err(win::Error::msg("CPU registry", "no logical processors"));
    }
    let mut name = None;
    for key in keys {
        if key.parse::<u32>().is_err() {
            return Err(win::Error::msg("CPU registry", "unknown processor subkey"));
        }
        let value = registry::read_string(&format!(r"{PROCESSORS}\{key}"), "ProcessorNameString")?;
        // WMI takes Name from this registry value. Do not trim brand padding or
        // substitute the firmware Version/CPUID brand, which can differ.
        if value.is_empty() || name.as_ref().is_some_and(|name| name != &value) {
            return Err(win::Error::msg(
                "CPU registry",
                "missing or differing logical-processor names; socket association needs WMI",
            ));
        }
        name = Some(value);
    }
    let name = name.ok_or_else(|| win::Error::msg("CPU registry", "missing processor name"))?;
    Ok((name, processor_id, serial))
}

fn firmware_processor(smbios: &firmware::Smbios) -> win::Result<(String, String)> {
    let missing = || win::Error::msg("SMBIOS type 4", "incomplete processor identity");
    let mut processor = None;
    for socket in smbios.structures.iter().filter(|entry| entry.kind == 4) {
        let status = socket.byte(0x18).ok_or_else(missing)?;
        if status & 0x40 == 0 {
            continue; // An explicitly unpopulated socket is not a processor.
        }
        // Firmware order is not a proven WMI row order, nor is a registry
        // logical-processor index a socket index. Keep WMI for multiple sockets
        // and disabled/unknown CPU states rather than changing their inventory.
        if processor.is_some() || status & 7 != 1 || socket.byte(5) != Some(3) {
            return Err(win::Error::msg(
                "SMBIOS type 4",
                "processor ordering or active socket association needs WMI",
            ));
        }
        let id = socket
            .qword(8)
            .filter(|id| !matches!(*id, 0 | u64::MAX))
            .ok_or_else(missing)?;
        let serial = socket.string(socket.byte(0x20).ok_or_else(missing)?);
        // Missing/index-zero strings cannot distinguish WMI null from empty.
        // Let WMI decide; retain every nonempty string, including OEM placeholders.
        if serial.is_empty() {
            return Err(missing());
        }
        // WMI's x64 ProcessorId is the little-endian type-4 qword in X16 form.
        // Live CPUID EDX:EAX differs on the owner PC and must never replace it.
        processor = Some((format!("{id:016X}"), serial.to_owned()));
    }
    processor.ok_or_else(missing)
}

fn write_processor(out: &mut Out, name: &str, processor_id: &str, serial: Option<&str>) {
    out.info("Name", name).info("ProcessorId", processor_id);
    if let Some(serial) = serial {
        out.info("SerialNumber", serial);
        if !serial.is_empty() {
            out.id_value(serial);
        }
    }
}

fn write_cpuid(
    out: &mut Out,
    leaf0: CpuidResult,
    leaf1: Option<CpuidResult>,
    leaf3: Option<CpuidResult>,
) {
    // C# parity: Hardware/CpuInfo.cs:35-45. Even with no WMI rows, the CPUID
    // block begins with a blank line; vendor bytes use EBX, EDX, ECX ASCII.
    let vendor: String = [leaf0.ebx, leaf0.edx, leaf0.ecx]
        .into_iter()
        .flat_map(u32::to_le_bytes)
        .map(|byte| {
            if byte.is_ascii() {
                char::from(byte)
            } else {
                '?'
            }
        })
        .collect();
    out.blank().info("CPUID Vendor", &vendor);
    if let Some(leaf) = leaf1 {
        let stepping = leaf.eax & 0xF;
        let base_model = (leaf.eax >> 4) & 0xF;
        let base_family = (leaf.eax >> 8) & 0xF;
        let ext_model = (leaf.eax >> 16) & 0xF;
        let ext_family = (leaf.eax >> 20) & 0xFF;
        // C# parity: Hardware/CpuInfo.cs:56-59. Extended model applies only
        // to base families 6/15; extended family applies only to base family 15.
        let family = if base_family == 0xF {
            base_family + ext_family
        } else {
            base_family
        };
        let model = if matches!(base_family, 0x6 | 0xF) {
            (ext_model << 4) | base_model
        } else {
            base_model
        };
        out.info(
            "CPUID Signature (decoded)",
            &format!("Family {family}, Model {model}, Stepping {stepping}"),
        );
    }
    if !leaf1.is_some_and(|leaf| leaf.edx & (1 << 18) != 0) {
        out.fallback_failed(
            "CPUID Serial Number",
            &win::Error::msg("CPUID", "PSN feature not supported"),
        );
    } else if let Some(leaf) = leaf3 {
        // Leaf 3 is a processor serial only when leaf 1 advertises PSN.
        let serial = (u64::from(leaf.edx) << 32) | u64::from(leaf.ecx);
        if serial != 0 {
            out.id("CPUID Serial Number", &format!("{serial:016X}"));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Deserialize;

    #[test]
    fn duplicate_socket_assets_and_old_versions_preserve_legacy_output() {
        let record = firmware::Structure {
            kind: 4,
            handle: 0x0418,
            formatted: vec![0; 0x23],
            strings: ["CPU1", "Intel(R) Corporation", "CPU-INV-2481"]
                .map(String::from)
                .to_vec(),
        };
        let mut table = firmware::Smbios {
            major: 3,
            minor: 6,
            structures: vec![record.clone(), record],
        };
        for (index, record) in table.structures.iter_mut().enumerate() {
            record.handle += index as u16;
            record.formatted[4] = 1;
            record.formatted[7] = 2;
            record.formatted[0x21] = 3;
        }
        let mut out = Out::new();
        out.info("Legacy", "kept");
        append_sockets(&table, &mut out);
        let section = out.finish();
        assert!(section.body.starts_with("Legacy: kept\r\n"));
        assert!(section.body.contains("CPU Socket #2 (SMBIOS)\r\n"));
        assert!(!section.body.contains("Socket Asset Tag (SMBIOS):"));
        assert!(
            section
                .failures
                .iter()
                .any(|failure| failure.contains("implausible"))
        );
        assert!(
            section
                .failures
                .iter()
                .all(|failure| !failure.contains("CPU-INV-2481"))
        );
        table.major = 2;
        table.minor = 2;
        for record in &mut table.structures {
            record.formatted.truncate(0x20);
        }
        let mut out = Out::new();
        append_sockets(&table, &mut out);
        let old = out.finish();
        assert!(old.body.contains("Socket Designation (SMBIOS): CPU1\r\n"));
        assert!(!old.body.contains("Part Number"));
        assert!(!old.body.contains("Unavailable"));
        assert!(
            old.failures
                .iter()
                .any(|failure| failure.contains("unsupported"))
        );
    }

    #[test]
    fn socket_rendering_uses_printable_socket_count_and_keeps_model_context_unmarked() {
        let mut record = firmware::Structure {
            kind: 4,
            handle: 0x0401,
            formatted: vec![0; 0x23],
            strings: [
                "CPU1",
                "Intel(R) Corporation",
                "BX8071512700K",
                "CPU-INV-2418",
            ]
            .map(String::from)
            .to_vec(),
        };
        let mut table = firmware::Smbios {
            major: 3,
            minor: 6,
            structures: vec![record.clone()],
        };
        let mut out = Out::new();
        append_sockets(&table, &mut out);
        let empty = out.finish();
        assert!(empty.body.is_empty());
        assert_eq!(empty.failures.len(), 1);
        for (offset, index) in [(4, 1), (7, 2), (0x22, 3), (0x21, 4)] {
            record.formatted[offset] = index;
        }
        table.structures.push(record.clone());
        let mut out = Out::new();
        append_sockets(&table, &mut out);
        let single = out.finish();
        assert!(
            single
                .body
                .starts_with("Socket Designation (SMBIOS): CPU1\r\n")
        );
        assert!(!single.body.contains("CPU Socket"));
        assert_eq!(single.ids, ["CPU-INV-2418"]);
        assert!(
            crate::report::masked(&single)
                .body
                .contains("BX8071512700K")
        );
        record.handle = 0x0402;
        record.formatted[0x21] = 255;
        table.structures.push(record);
        let mut out = Out::new();
        append_sockets(&table, &mut out);
        let multiple = out.finish();
        assert!(multiple.body.starts_with("CPU Socket #1 (SMBIOS)\r\n"));
        assert!(multiple.body.contains("CPU Socket #2 (SMBIOS)\r\n"));
        assert!(!multiple.body.contains("0x040"));
        assert!(
            multiple
                .body
                .contains("Socket Asset Tag (SMBIOS): Unavailable (")
        );
    }

    #[derive(Deserialize)]
    #[serde(rename_all = "PascalCase")]
    struct FixtureProcessor {
        name: Option<String>,
        processor_id: Option<String>,
        serial_number: Option<String>,
    }

    #[derive(Deserialize)]
    struct FixtureSocket {
        formatted: String,
        strings: Vec<String>,
    }

    #[test]
    fn fabricated_processors_keep_null_empty_placeholder_and_native_serial_text() {
        let rows: Vec<FixtureProcessor> =
            serde_json::from_str(include_str!("../../tests/fixtures/wp-04/cpu-wmi.json"))
                .expect("fabricated CPU fixture");
        let mut out = Out::new();
        for row in rows {
            write_processor(
                &mut out,
                &row.name.unwrap_or_default(),
                &row.processor_id.unwrap_or_default(),
                row.serial_number.as_deref(),
            );
        }
        write_cpuid(
            &mut out,
            CpuidResult {
                eax: 3,
                ebx: u32::from_le_bytes(*b"Auth"),
                edx: u32::from_le_bytes(*b"enti"),
                ecx: u32::from_le_bytes(*b"cAMD"),
            },
            Some(CpuidResult {
                eax: 0x008F_1F42,
                ebx: 0,
                ecx: 0,
                edx: 1 << 18,
            }),
            Some(CpuidResult {
                eax: 0,
                ebx: 0,
                ecx: 0x6B3C_2A91,
                edx: 0xD48E_7F15,
            }),
        );
        let section = out.finish();
        let expected: String =
            serde_json::from_str(include_str!("../../tests/fixtures/wp-04/cpu-text.json"))
                .expect("legacy CPU text with escaped CRLF");
        assert_eq!(section.body.as_bytes(), expected.as_bytes());
        assert_eq!(section.ids, ["To Be Filled By O.E.M.", "D48E7F156B3C2A91"]);
        let masked = crate::report::masked(&section);
        assert!(masked.body.contains("SerialNumber: XX XX XXXXXX XX X.X.X."));
        assert!(masked.body.contains("ProcessorId: 00AF0764C1EBFA2B"));
        let mut out = Out::new();
        write_processor(&mut out, "", "", Some(""));
        let empty = out.finish();
        assert_eq!(empty.body, "Name: \r\nProcessorId: \r\nSerialNumber: \r\n");
        assert!(empty.ids.is_empty());

        // Extend the CPU fixture contract with fabricated firmware bytes. This
        // covers input formats and fallback decisions unavailable on this PC.
        let sockets: Vec<FixtureSocket> =
            serde_json::from_str(include_str!("../../tests/fixtures/wp-04/cpu-smbios.json"))
                .expect("fabricated type-4 sockets");
        let mut smbios = firmware::Smbios {
            major: 3,
            minor: 0,
            structures: sockets
                .into_iter()
                .map(|socket| firmware::Structure {
                    kind: 4,
                    handle: 0,
                    formatted: socket
                        .formatted
                        .split_whitespace()
                        .map(|byte| u8::from_str_radix(byte, 16).expect("fixture byte"))
                        .collect(),
                    strings: socket.strings,
                })
                .collect(),
        };
        assert_eq!(
            firmware_processor(&smbios).expect("one populated socket"),
            (
                "00AF0764C1EBFA2B".to_owned(),
                "To Be Filled By O.E.M.".to_owned()
            )
        );
        let complete = smbios.structures[0].clone();
        for length in 0..=0x20 {
            smbios.structures[0] = complete.clone();
            smbios.structures[0].formatted.truncate(length);
            assert!(
                firmware_processor(&smbios).is_err(),
                "truncated at {length}"
            );
        }
        for index in [0, 5, 255] {
            smbios.structures[0] = complete.clone();
            smbios.structures[0].formatted[0x20] = index;
            assert!(firmware_processor(&smbios).is_err(), "serial index {index}");
        }
        for sentinel in [0, 255] {
            smbios.structures[0] = complete.clone();
            smbios.structures[0].formatted[8..16].fill(sentinel);
            assert!(firmware_processor(&smbios).is_err());
        }
        for status in [0, 0x40, 0x42, 0x43, 0x44] {
            smbios.structures[0] = complete.clone();
            smbios.structures[0].formatted[0x18] = status;
            assert!(firmware_processor(&smbios).is_err(), "status {status}");
        }
        smbios.structures[0] = complete.clone();
        smbios.structures.push(complete);
        assert!(
            firmware_processor(&smbios).is_err(),
            "multi-socket order needs WMI"
        );
        smbios.structures.clear();
        assert!(firmware_processor(&smbios).is_err());
    }

    #[test]
    fn cpuid_signature_ignores_reserved_extensions_and_omits_zero_serial() {
        let leaf0 = CpuidResult {
            eax: 3,
            ebx: u32::from_le_bytes(*b"Genu"),
            edx: u32::from_le_bytes(*b"ineI"),
            ecx: u32::from_le_bytes(*b"ntel"),
        };
        for (eax, signature) in [
            (0x007A_05A7, "Family 5, Model 10, Stepping 7"),
            (0x0009_06E3, "Family 6, Model 158, Stepping 3"),
        ] {
            let mut out = Out::new();
            write_cpuid(
                &mut out,
                leaf0,
                Some(CpuidResult {
                    eax,
                    ebx: 0,
                    ecx: 0,
                    edx: 0,
                }),
                Some(CpuidResult {
                    eax: 0,
                    ebx: 0,
                    ecx: 0,
                    edx: 0,
                }),
            );
            let section = out.finish();
            assert_eq!(
                section.body,
                format!(
                    "\r\nCPUID Vendor: GenuineIntel\r\nCPUID Signature (decoded): {signature}\r\n"
                )
            );
            assert!(section.ids.is_empty());
        }
        for edx in [0, 1 << 18] {
            let mut out = Out::new();
            write_cpuid(
                &mut out,
                leaf0,
                Some(CpuidResult {
                    eax: 0,
                    ebx: 0,
                    ecx: 0,
                    edx,
                }),
                Some(CpuidResult {
                    eax: 0,
                    ebx: 0,
                    ecx: 0x6B3C_2A91,
                    edx: 0xD48E_7F15,
                }),
            );
            let section = out.finish();
            assert_eq!(section.body.contains("CPUID Serial Number:"), edx != 0);
            assert_eq!(section.failures.iter().any(|f| f.contains("PSN")), edx == 0);
        }
        let mut out = Out::new();
        write_cpuid(&mut out, leaf0, None, None);
        assert_eq!(out.finish().body, "\r\nCPUID Vendor: GenuineIntel\r\n");
    }

    #[test]
    #[ignore = "read-only owner-PC CPU capture; redirect output to private golden/wp-04"]
    fn wp04_capture_cpu() {
        let sections = crate::hw::collect_all(Some("CPU"), &|_, _| {});
        let section = sections.first().expect("CPU provider is registered");
        println!("WP04_CAPTURE_BEGIN CPU");
        print!("{}", section.body);
        println!("WP04_CAPTURE_END CPU");
        println!(
            "elapsed_ms={} source={} failures={:?}",
            section.elapsed_ms, section.source, section.failures
        );
        assert!(section.failures.is_empty(), "CPU collection failed");
    }
}
