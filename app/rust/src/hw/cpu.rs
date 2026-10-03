//! Legacy WMI processor fields and native CPUID identifiers.

use crate::{
    hw::Ctx,
    report::Out,
    win::{self, wmi},
};
use core::arch::x86_64::{__cpuid, CpuidResult};

/// Collects this hardware section through the shared output builder.
pub fn collect(_ctx: &Ctx, out: &mut Out) -> Result<(), win::Error> {
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
    // x86-64 guarantees CPUID. The pinned Rust 1.98 intrinsics are safe, so the
    // provider needs neither unsafe nor a second Win32 helper implementation.
    let leaf0 = __cpuid(0);
    // C# parity: Hardware/CpuInfo.cs:38,47,62. maxLeaf is a signed Int32.
    let leaf1 = (leaf0.eax as i32 >= 1).then(|| __cpuid(1));
    let leaf3 = (leaf0.eax as i32 >= 3).then(|| __cpuid(3));
    write_cpuid(out, leaf0, leaf1, leaf3);
    // The shared collector renders and records a WMI failure after retaining
    // the independent native data; no failure is silently discarded. Partial
    // CPU output on this failure path needs an orchestrator approval-ledger row.
    result.map(|_| ())
}

fn write_processor(out: &mut Out, name: &str, processor_id: &str, serial: Option<&str>) {
    out.info("Name", name).id("ProcessorId", processor_id);
    if let Some(serial) = serial {
        out.id("SerialNumber", serial);
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
    if let Some(leaf) = leaf3 {
        // C# parity: Hardware/CpuInfo.cs:62-67. There is deliberately no PSN
        // feature-bit check; X16 keeps the same bits even for a signed long.
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

    #[derive(Deserialize)]
    #[serde(rename_all = "PascalCase")]
    struct FixtureProcessor {
        name: Option<String>,
        processor_id: Option<String>,
        serial_number: Option<String>,
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
                edx: 0,
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
        assert_eq!(
            section.ids,
            [
                "00AF0764C1EBFA29",
                "00AF0764C1EBFA2A",
                "",
                "00AF0764C1EBFA2B",
                "To Be Filled By O.E.M.",
                "D48E7F156B3C2A91",
            ]
        );
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
