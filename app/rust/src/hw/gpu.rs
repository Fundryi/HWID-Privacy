//! GPU identities, with NVIDIA first and the C# WMI tree for other adapters.

use crate::{
    hw::{Ctx, first_ok},
    report::{self, Out},
    win::{self, nvidia, process, wmi},
};

#[derive(Clone)]
struct Adapter {
    name: String,
    pnp_id: String,
    hardware_id: Option<String>,
    pci_address: Option<nvidia::PciAddress>,
}

/// Collects all GPU identities through optional NVIDIA APIs and cached SetupAPI IDs.
pub fn collect(ctx: &Ctx, out: &mut Out) -> Result<(), win::Error> {
    let source = std::cell::Cell::new("");
    let smi_seen = std::cell::RefCell::new(std::collections::HashSet::new());
    let smi_start = std::cell::Cell::new(None);
    let smi = |path: std::path::PathBuf| {
        let start = smi_start.get().unwrap_or_else(|| {
            let start = std::time::Instant::now();
            smi_start.set(Some(start));
            start
        });
        let resolved = std::fs::canonicalize(&path)
            .map_err(|e| win::Error::msg("nvidia-smi path", e.to_string()))?;
        // Windows paths are case-insensitive; canonicalize resolves directory links too.
        if !smi_seen
            .borrow_mut()
            .insert(resolved.as_os_str().to_ascii_lowercase())
        {
            return Err(win::Error::msg(
                "nvidia-smi path",
                "resolved executable already attempted",
            ));
        }
        let budget = std::time::Duration::from_secs(15).saturating_sub(start.elapsed());
        if budget.is_zero() {
            return Err(win::Error::msg(
                "nvidia-smi",
                "shared fifteen-second deadline expired",
            ));
        }
        nvidia::smi(&resolved, budget)
    };
    let nvml_system = || {
        let capture = nvidia::nvml(&process::system32("nvml.dll"))?;
        source.set("NVML (System32)");
        Ok(capture)
    };
    let nvml_standard = || {
        let capture = nvidia::nvml(&nvidia::standard_path("nvml.dll")?)?;
        source.set("NVML (NVSMI)");
        Ok(capture)
    };
    let smi_system = || {
        let capture = smi(process::system32("nvidia-smi.exe"))?;
        source.set("nvidia-smi (System32)");
        Ok(capture)
    };
    let smi_standard = || {
        let capture = smi(nvidia::standard_path("nvidia-smi.exe")?)?;
        source.set("nvidia-smi (NVSMI)");
        Ok(capture)
    };
    // C# parity: Hardware/GpuInfo.cs:24-83. Any NVIDIA identity failure falls
    // back to Win32_VideoController; successful fallbacks stay diagnostics-only.
    let native = first_ok(
        out,
        "NVIDIA GPUs",
        &[
            ("NVML (System32)", &nvml_system),
            ("NVML (NVSMI)", &nvml_standard),
            ("nvidia-smi (System32)", &smi_system),
            ("nvidia-smi (NVSMI)", &smi_standard),
        ],
    );
    let native = match native {
        Ok(capture) => {
            record_failures(out, &capture.failures);
            Some(capture.items)
        }
        Err(_) => None, // first_ok already recorded every failed source.
    };
    // The shared collection worker bounds this WMI call at sixty seconds.
    let rows = wmi::query(wmi::Namespace::Cimv2, "SELECT * FROM Win32_VideoController");
    let mut wmi_error = None;
    let mut adapters = match rows {
        Ok(rows) => rows
            .into_iter()
            .map(|row| Adapter {
                // C# parity: Hardware/GpuInfo.cs:100-101. Null alone becomes Unknown.
                name: row.str("Name").unwrap_or_else(|| "Unknown".to_owned()),
                pnp_id: row
                    .str("PNPDeviceID")
                    .unwrap_or_else(|| "Unknown".to_owned()),
                hardware_id: None,
                pci_address: None,
            })
            .collect::<Vec<_>>(),
        Err(error) if native.is_some() => {
            wmi_error = Some(error);
            Vec::new()
        }
        Err(error) => return Err(error),
    };
    let mut nvidia_adapters = Vec::new();
    if let Some(gpus) = &native {
        let is_nvidia = |adapter: &Adapter| {
            adapter.pnp_id.to_ascii_uppercase().contains("VEN_10DE")
                || (adapter.pnp_id == "Unknown" && adapter.name.starts_with("NVIDIA "))
        };
        nvidia_adapters = adapters
            .iter()
            .filter(|adapter| is_nvidia(adapter))
            .cloned()
            .collect();
        if !nvidia_adapters.is_empty() {
            let ids: Vec<_> = nvidia_adapters
                .iter()
                .map(|adapter| adapter.pnp_id.as_str())
                .collect();
            match nvidia::adapter_pci_addresses(&ids) {
                Ok(capture) => {
                    record_failures(out, &capture.failures);
                    for (index, address) in capture.items {
                        nvidia_adapters[index].pci_address = Some(address);
                    }
                }
                Err(error) => {
                    out.fallback_failed("WMI PCI match", &error);
                }
            }
        }
        // Same NVIDIA count on both sides: every WMI NVIDIA row is in the
        // NVIDIA block, even when the driver names differ (e.g. no "NVIDIA "
        // prefix in older NVML names), so none is listed twice.
        let all_represented = adapters.iter().filter(|a| is_nvidia(a)).count() == gpus.len();
        let mut represented = vec![false; gpus.len()];
        adapters.retain(|adapter| {
            // Otherwise consume one matching NVIDIA name per physical GPU, retaining
            // any unmatched adapter. Never hide all VEN_10DE rows on a partial list.
            let is_nvidia = is_nvidia(adapter);
            if is_nvidia && all_represented {
                return false;
            }
            let matched = gpus.iter().enumerate().position(|(i, gpu)| {
                is_nvidia && !represented[i] && report::eq_ignore_case(&adapter.name, &gpu.name)
            });
            match matched {
                Some(i) => {
                    represented[i] = true;
                    false
                }
                None => true,
            }
        });
    }
    if !adapters.is_empty() {
        // C# parity: Hardware/GpuInfo.cs:94-95,107-114. The PNP line is still
        // usable when the hardware map fails; its error goes to diagnostics.
        match ctx.hardware_ids() {
            Ok(map) => {
                for adapter in &mut adapters {
                    adapter.hardware_id = map.get(&adapter.pnp_id.to_uppercase()).cloned();
                }
            }
            Err(error) => {
                out.fallback_failed("SetupAPI hardware IDs", &error);
            }
        }
    }
    let boards = if native.is_some() {
        match nvidia::boards() {
            Ok(capture) => {
                record_failures(out, &capture.failures);
                Some(capture)
            }
            Err(error) => {
                out.fallback_failed("NVAPI board", &error);
                // AD-03: there is no fallback source for the missing board item.
                // Defer this line until after the intact NVIDIA identity block.
                Some(nvidia::Capture {
                    items: Vec::new(),
                    failures: vec![("NVAPI board", error)],
                })
            }
        }
    } else {
        None
    };
    render(
        out,
        native.as_deref().unwrap_or(&[]),
        boards.as_ref(),
        &adapters,
        &nvidia_adapters,
    );
    if native.is_none() {
        out.source("WMI + SetupAPI");
    } else {
        // first_ok recorded the chosen NVIDIA source; retain it in diagnostics
        // along with enrichment provenance rather than replacing it with WMI.
        out.source(&format!(
            "{} + NVAPI; WMI + SetupAPI for additional adapters",
            source.get()
        ));
    }
    if let Some(error) = wmi_error {
        out.fallback_failed("WMI additional adapters", &error)
            .blank()
            .text(&format!("WMI query failed: Win32_VideoController: {error}"));
    }
    // C# parity: Hardware/GpuInfo.cs:80,124. Trim any trailing enrichment error too.
    out.trim_end();
    Ok(())
}

fn record_failures(out: &mut Out, failures: &[(&str, win::Error)]) {
    for (source, error) in failures {
        out.fallback_failed(source, error);
    }
}

fn matched_board<'a>(
    gpu: &nvidia::Gpu,
    gpus: &[nvidia::Gpu],
    boards: &'a [nvidia::Board],
) -> Option<&'a nvidia::Board> {
    let bus = gpu.pci_bus?;
    // A bus is sufficient only when unique on both complete enumerations.
    // NVML/SMI nonzero PCI domains are rejected in the Win32 boundary.
    if gpus.len() != boards.len() || gpus.iter().any(|gpu| gpu.pci_bus.is_none()) {
        return None;
    }
    if gpus.iter().filter(|g| g.pci_bus == Some(bus)).count() != 1 {
        return None;
    }
    let mut candidates = boards.iter().filter(|b| b.pci_bus == bus);
    let matched = candidates.next()?;
    if candidates.next().is_some() {
        None
    } else {
        Some(matched)
    }
}

fn render(
    out: &mut Out,
    gpus: &[nvidia::Gpu],
    boards: Option<&nvidia::Capture<nvidia::Board>>,
    adapters: &[Adapter],
    nvidia_adapters: &[Adapter],
) {
    for gpu in gpus {
        // C# parity: Hardware/GpuInfo.cs:52-65. NVIDIA GPU groups are adjacent;
        // the UUID stays the last tree leaf even when a board line follows.
        out.text(&format!("GPU {}", gpu.index))
            .text(&format!("└── {}", gpu.name));
        if let Some(suffix) = &gpu.uuid_suffix {
            out.text(&format!("    └── UUID{suffix}"));
            let uuid = report::trim_net(suffix.strip_prefix(':').unwrap_or(suffix));
            out.id_value(uuid);
        }
    }
    // AD-15/F-07a: keep details after all NVIDIA groups, in GPU index order.
    let mut detail_line = false;
    for gpu in gpus {
        let prefix = if gpus.len() == 1 {
            String::new()
        } else {
            format!("GPU {} ", gpu.index)
        };
        if let Some(boards) = boards {
            // A failed bus/handle makes identity incomplete. Optional board failures
            // retain their successfully identified buses and do not hide other boards.
            let complete = boards
                .failures
                .iter()
                .all(|(_, error)| matches!(error.op, "NvAPI_Unload" | "NvAPI_GPU_GetBoardInfo"));
            // Preserve the single-GPU board contract even if WMI enrichment fails.
            // Multi-GPU output additionally requires a complete, unique BDF join to WMI.
            let wmi_matched = gpus.len() == 1
                || (nvidia_adapters.len() == gpus.len()
                    && nvidia_adapters
                        .iter()
                        .all(|adapter| adapter.pci_address.is_some())
                    && gpus.iter().all(|gpu| {
                        gpu.pci_address.is_some_and(|address| {
                            nvidia_adapters
                                .iter()
                                .filter(|adapter| adapter.pci_address == Some(address))
                                .count()
                                == 1
                        })
                    }));
            if let Some(board) = (complete && wmi_matched)
                .then(|| matched_board(gpu, gpus, &boards.items))
                .flatten()
            {
                if let Some(value) = nvidia::board_value(&board.bytes) {
                    // C# parity: Hardware/GpuInfo.cs:74-75. AD-12 changes only
                    // the label; AD-13 changes binary values to separator-free hex.
                    if !detail_line {
                        out.blank();
                        detail_line = true;
                    }
                    out.text(&format!("{prefix}Board Serial Number: {value}"))
                        .id_value(&value);
                }
            } else {
                // AD-15 permits omitting an unproven board match; retain evidence.
                out.fallback_failed(
                    "NVAPI PCI match",
                    &win::Error::msg(
                        "NVAPI PCI match",
                        format!("GPU {}: unique PCI bus match not proven", gpu.index),
                    ),
                );
            }
        }
        for (label, value) in [
            ("Serial Number", &gpu.serial),
            ("PDI", &gpu.pdi),
            ("Board Part Number", &gpu.board_part),
        ] {
            if let Some(value) = value {
                if !detail_line {
                    out.blank();
                    detail_line = true;
                }
                out.text(&format!("{prefix}{label}: {value}"))
                    .id_value(value);
            }
        }
        if let Some(value) = &gpu.vbios {
            if !detail_line {
                out.blank();
                detail_line = true;
            }
            out.text(&format!("{prefix}VBIOS Version: {value}"));
        }
    }
    if let Some(boards) = boards {
        for (_, error) in &boards.failures {
            if error.op != "NvAPI_Unload" {
                // AD-15/46: the failed board occupies the same place as a board value.
                out.blank()
                    .text(&format!("Board Serial Number: Unavailable ({error})"));
            }
        }
    }
    for (position, adapter) in adapters.iter().enumerate() {
        if position > 0 || !gpus.is_empty() {
            out.blank();
        }
        // C# parity: Hardware/GpuInfo.cs:103-120. WMI order, tree, and blank lines.
        out.text(&format!("GPU {}", gpus.len() + position))
            .text(&format!("└── {}", adapter.name));
        match &adapter.hardware_id {
            Some(id) => {
                out.text(&format!("    ├── {}", adapter.pnp_id))
                    .text(&format!("    └── Hardware ID: {id}"))
                    .id_value(id);
            }
            None => {
                out.text(&format!("    └── {}", adapter.pnp_id));
            }
        }
        out.id_value(&adapter.pnp_id);
    }
    if gpus.is_empty() && adapters.is_empty() {
        // C# parity: Hardware/GpuInfo.cs:89-92.
        out.text("No GPU detected.");
    }
    // C# parity: Hardware/GpuInfo.cs:80,124. Both identity paths use TrimEnd.
    out.trim_end();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gpu_text_preserves_legacy_trees_and_only_approved_additions() {
        let gpu = nvidia::Gpu {
            index: 0,
            name: "NVIDIA GeForce RTX 5080".to_owned(),
            uuid_suffix: Some(": GPU-358d91ef-2174-43eb-9221-d36a721cd603".to_owned()),
            pci_bus: Some(1),
            ..Default::default()
        };
        let board = nvidia::Capture {
            items: vec![nvidia::Board {
                pci_bus: 1,
                bytes: *b"042571983612\0\0\0\0",
            }],
            failures: vec![],
        };
        let adapter = Adapter {
            name: "Intel(R) UHD Graphics 770".to_owned(),
            pnp_id: "PCI\\VEN_8086&DEV_A780\\3&24137E09&0&10".to_owned(),
            hardware_id: Some("PCI\\VEN_8086&DEV_A780&SUBSYS_88881043&REV_04".to_owned()),
            pci_address: None,
        };
        let mut out = Out::new();
        render(
            &mut out,
            std::slice::from_ref(&gpu),
            Some(&board),
            &[adapter],
            &[],
        );
        let section = out.finish();
        assert_eq!(
            section.body,
            include_str!("../../tests/fixtures/wp-07/mixed-gpus.fixture")
                .replace("\r\n", "\n")
                .replace('\n', "\r\n")
                .trim_end()
        );
        assert_eq!(section.ids.len(), 4);

        let mut out = Out::new();
        render(
            &mut out,
            &[],
            None,
            &[Adapter {
                name: "Unknown".to_owned(),
                pnp_id: "Unknown".to_owned(),
                hardware_id: None,
                pci_address: None,
            }],
            &[],
        );
        assert_eq!(out.finish().body, "GPU 0\r\n└── Unknown\r\n    └── Unknown");
        let mut out = Out::new();
        render(&mut out, &[], None, &[], &[]);
        assert_eq!(out.finish().body, "No GPU detected.");
        let mut out = Out::new();
        render(
            &mut out,
            &[gpu],
            Some(&nvidia::Capture {
                items: vec![],
                failures: vec![(
                    "NVAPI board",
                    win::Error::msg("NvAPI_GPU_GetBoardInfo", "fabricated failure"),
                )],
            }),
            &[],
            &[],
        );
        assert!(out.finish().body.ends_with(
            "\r\n\r\nBoard Serial Number: Unavailable (NvAPI_GPU_GetBoardInfo failed: 0x00000000 fabricated failure)"
        ));
    }

    #[test]
    fn gpu_board_match_refuses_ambiguous_or_missing_buses() {
        let gpu = |index, pci_bus| nvidia::Gpu {
            index,
            name: "NVIDIA".to_owned(),
            uuid_suffix: None,
            pci_bus,
            pci_address: pci_bus.map(|bus| nvidia::PciAddress {
                bus,
                device: 0,
                function: 0,
            }),
            ..Default::default()
        };
        let gpus = [gpu(0, Some(2)), gpu(1, Some(3))];
        let boards = [
            nvidia::Board {
                pci_bus: 3,
                bytes: [b'1'; 16],
            },
            nvidia::Board {
                pci_bus: 2,
                bytes: [b'2'; 16],
            },
        ];
        assert_eq!(
            matched_board(&gpus[0], &gpus, &boards).map(|b| b.bytes),
            Some([b'2'; 16])
        );
        assert!(matched_board(&gpu(0, None), &gpus, &boards).is_none());
        assert!(matched_board(&gpus[0], &[gpu(0, Some(2)), gpu(1, None)], &boards).is_none());
        assert!(matched_board(&gpus[0], &gpus, &boards[..1]).is_none());
        assert!(matched_board(&gpus[0], &[gpu(0, Some(2)), gpu(1, Some(2))], &boards).is_none());
        assert!(
            matched_board(
                &gpus[0],
                &gpus,
                &[
                    nvidia::Board {
                        pci_bus: 2,
                        bytes: [1; 16]
                    },
                    nvidia::Board {
                        pci_bus: 2,
                        bytes: [2; 16]
                    }
                ]
            )
            .is_none()
        );
        // AD-15/F-07a: per-GPU lines follow both groups, regardless of NVAPI order.
        let mut out = Out::new();
        render(
            &mut out,
            &gpus,
            Some(&nvidia::Capture {
                items: boards.to_vec(),
                failures: vec![],
            }),
            &[],
            &gpus
                .iter()
                .map(|gpu| Adapter {
                    name: "NVIDIA".to_owned(),
                    pnp_id: "Unknown".to_owned(),
                    hardware_id: None,
                    pci_address: gpu.pci_address,
                })
                .collect::<Vec<_>>(),
        );
        assert_eq!(
            out.finish().body,
            "GPU 0\r\n└── NVIDIA\r\nGPU 1\r\n└── NVIDIA\r\n\r\nGPU 0 Board Serial Number: 2222222222222222\r\nGPU 1 Board Serial Number: 1111111111111111"
        );
    }

    #[test]
    #[ignore = "read-only owner-PC capture; prints private hardware identifiers"]
    fn wp07_capture_gpu() {
        let provider = crate::hw::Provider {
            title: "GPU INFO",
            collect,
        };
        for run in 1..=5 {
            let section = crate::hw::collect_provider(&provider, &Ctx::new());
            println!(
                "RUN {run}: {} ms; admin={}\nSOURCE: {}\nBEGIN GPU INFO\n{}END GPU INFO\nFAILURES: {:?}",
                section.elapsed_ms,
                win::security::is_admin(),
                section.source,
                section.body,
                section.failures
            );
            assert!(!section.body.contains("not ported yet"));
        }
        // Independent fallback cross-check, using the same bounded process helper.
        match nvidia::smi(
            &process::system32("nvidia-smi.exe"),
            std::time::Duration::from_secs(15),
        ) {
            Ok(capture) => {
                for gpu in &capture.items {
                    println!(
                        "SMI GPU {}: {} {:?}; PCI bus {:?}",
                        gpu.index, gpu.name, gpu.uuid_suffix, gpu.pci_bus
                    );
                }
                println!("SMI failures: {:?}", capture.failures);
                if let Ok(native) = nvidia::nvml(&process::system32("nvml.dll")) {
                    let identities = |gpus: &[nvidia::Gpu]| {
                        gpus.iter()
                            .map(|gpu| {
                                (
                                    gpu.index,
                                    gpu.name.clone(),
                                    gpu.uuid_suffix.clone(),
                                    gpu.pci_bus,
                                )
                            })
                            .collect::<Vec<_>>()
                    };
                    assert_eq!(identities(&native.items), identities(&capture.items));
                    println!(
                        "PARITY: NVML and nvidia-smi indices, names, UUID text and PCI buses are identical."
                    );
                }
            }
            Err(error) => println!("SMI check failed: {error}"),
        }
        match nvidia::boards() {
            Ok(capture) => {
                for board in capture.items {
                    println!(
                        "NVAPI RAW: PCI bus {}; bytes {:02X?}; usable={:?}",
                        board.pci_bus,
                        board.bytes,
                        nvidia::board_value(&board.bytes)
                    );
                }
                println!("NVAPI failures: {:?}", capture.failures);
            }
            Err(error) => println!("NVAPI check failed: {error}"),
        }
        match wmi::query(wmi::Namespace::Cimv2, "SELECT * FROM Win32_VideoController") {
            Ok(rows) => {
                let ctx = Ctx::new();
                let adapters: Vec<_> = rows
                    .into_iter()
                    .map(|row| {
                        let pnp_id = row
                            .str("PNPDeviceID")
                            .unwrap_or_else(|| "Unknown".to_owned());
                        Adapter {
                            name: row.str("Name").unwrap_or_else(|| "Unknown".to_owned()),
                            hardware_id: ctx.hardware_id(&pnp_id).map(str::to_owned),
                            pci_address: None,
                            pnp_id,
                        }
                    })
                    .collect();
                let mut out = Out::new();
                render(&mut out, &[], None, &adapters, &[]);
                println!(
                    "BEGIN WMI-ONLY GPU INFO\n{}END WMI-ONLY GPU INFO",
                    out.finish().body
                );
            }
            Err(error) => println!("WMI check failed: {error}"),
        }
    }
}
