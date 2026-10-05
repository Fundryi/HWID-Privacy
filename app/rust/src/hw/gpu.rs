//! GPU identities, with NVIDIA first and the C# WMI tree for other adapters.

use crate::{
    hw::{Ctx, first_ok},
    report::{self, Out},
    win::{self, nvidia, process, wmi},
};

fn smi_budget(
    resolved: &std::path::Path,
    seen: &mut std::collections::HashSet<std::ffi::OsString>,
    elapsed: std::time::Duration,
) -> Result<std::time::Duration, win::Error> {
    if !seen.insert(resolved.as_os_str().to_ascii_lowercase()) {
        return Err(win::Error::msg(
            "nvidia-smi path",
            "absent: resolved executable already attempted",
        ));
    }
    let budget = std::time::Duration::from_secs(15).saturating_sub(elapsed);
    if budget.is_zero() {
        Err(win::Error::msg(
            "nvidia-smi",
            "timeout: shared fifteen-second deadline expired",
        ))
    } else {
        Ok(budget)
    }
}

fn is_nvidia(adapter: &Adapter) -> bool {
    adapter.pnp_id.to_ascii_uppercase().contains("VEN_10DE")
        || (adapter.pnp_id == "Unknown" && adapter.name.starts_with("NVIDIA "))
}

fn retain_additional(out: &mut Out, adapters: &mut Vec<Adapter>, gpus: &[nvidia::Gpu]) {
    let nvidia_rows: Vec<_> = adapters.iter().filter(|a| is_nvidia(a)).cloned().collect();
    if nvidia_rows.len() != gpus.len() {
        out.fallback_failed(
            "NVIDIA/WMI count",
            &win::Error::msg("NVIDIA/WMI count", "implausible: GPU counts disagree"),
        );
    }
    adapters.retain(|adapter| {
        if !is_nvidia(adapter) {
            return true;
        }
        let proven = adapter.pci_address.is_some_and(|address| {
            gpus.iter()
                .filter(|gpu| gpu.pci_address == Some(address))
                .count()
                == 1
                && nvidia_rows
                    .iter()
                    .filter(|row| row.pci_address == Some(address))
                    .count()
                    == 1
        });
        if !proven {
            out.fallback_failed(
                "NVIDIA/WMI adapter match",
                &win::Error::msg(
                    "NVIDIA/WMI adapter match",
                    "ambiguous: exact PCI BDF join not proven",
                ),
            );
        }
        !proven
    });
}

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
        let resolved = std::fs::canonicalize(&path).map_err(|e| {
            nvidia::classified(win::Error {
                op: "nvidia-smi path",
                code: e.raw_os_error().unwrap_or_default() as u32,
                detail: "executable path resolution failed".to_owned(),
            })
        })?;
        // Windows paths are case-insensitive; canonicalize resolves directory links too.
        let budget = smi_budget(&resolved, &mut smi_seen.borrow_mut(), start.elapsed())?;
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
        Ok(rows) => {
            if rows.len() > 256 {
                out.fallback_failed(
                    "WMI GPUs",
                    &win::Error::msg("WMI GPUs", "implausible: GPU count exceeds cap"),
                );
            }
            rows.into_iter()
                .take(256)
                .map(|row| Adapter {
                    // C# parity: Hardware/GpuInfo.cs:100-101. Null alone becomes Unknown.
                    name: row.str("Name").unwrap_or_else(|| "Unknown".to_owned()),
                    pnp_id: row
                        .str("PNPDeviceID")
                        .unwrap_or_else(|| "Unknown".to_owned()),
                    hardware_id: None,
                    pci_address: None,
                })
                .collect::<Vec<_>>()
        }
        Err(error) if native.is_some() => {
            wmi_error = Some(error);
            Vec::new()
        }
        Err(error) => return Err(error),
    };
    let mut nvidia_adapters = Vec::new();
    if let Some(gpus) = &native {
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
                        if let Some(adapter) = nvidia_adapters.get_mut(index) {
                            adapter.pci_address = Some(address);
                        } else {
                            out.fallback_failed(
                                "WMI PCI match",
                                &win::Error::msg(
                                    "WMI PCI match",
                                    "malformed: adapter index outside snapshot",
                                ),
                            );
                        }
                    }
                }
                Err(error) => {
                    out.fallback_failed("WMI PCI match", &error);
                }
            }
        }
        for adapter in &mut adapters {
            let mut matches = nvidia_adapters
                .iter()
                .filter(|row| row.pnp_id == adapter.pnp_id);
            if let Some(row) = matches.next().filter(|_| matches.next().is_none()) {
                adapter.pci_address = row.pci_address;
            }
        }
        retain_additional(out, &mut adapters, gpus);
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
            Ok(capture) => Some(capture),
            Err(error) => {
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
        out.fallback_failed(source, &nvidia::classified(error.clone()));
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
    if gpus.len() != boards.len()
        || gpus.iter().any(|gpu| gpu.pci_bus.is_none())
        || gpus.iter().any(|gpu| {
            gpus.iter()
                .filter(|other| other.pci_bus == gpu.pci_bus)
                .count()
                != 1
        })
        || boards.iter().any(|board| {
            boards
                .iter()
                .filter(|other| other.pci_bus == board.pci_bus)
                .count()
                != 1
        })
        || !gpus.iter().all(|gpu| {
            boards
                .iter()
                .any(|board| Some(board.pci_bus) == gpu.pci_bus)
        })
    {
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
    if let Some(boards) = boards {
        record_failures(out, &boards.failures);
        if boards.items.len() != gpus.len() {
            out.fallback_failed(
                "NVIDIA/NVAPI count",
                &win::Error::msg("NVIDIA/NVAPI count", "implausible: GPU counts disagree"),
            );
        }
    }
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
            let wmi_matched = (gpus.len() == 1 && nvidia_adapters.len() <= 1)
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
                    let duplicate = boards
                        .items
                        .iter()
                        .filter_map(|other| nvidia::board_value(&other.bytes))
                        .filter(|other| other.eq_ignore_ascii_case(&value))
                        .count()
                        > 1;
                    if duplicate {
                        out.fallback_failed(
                            "NVAPI board",
                            &win::Error::msg(
                                "NVAPI board",
                                "implausible: duplicate board serial across GPUs",
                            ),
                        );
                    } else {
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
                    let class = if board
                        .bytes
                        .iter()
                        .all(|byte| matches!(*byte, 0 | b'0' | b' '))
                    {
                        "placeholder"
                    } else {
                        "implausible"
                    };
                    out.fallback_failed(
                        "NVAPI board",
                        &win::Error::msg(
                            "NVAPI board",
                            format!("{class}: empty, zero-only or repeated board bytes"),
                        ),
                    );
                }
            } else {
                // AD-15 permits omitting an unproven board match; retain evidence.
                out.fallback_failed(
                    "NVAPI PCI match",
                    &win::Error::msg(
                        "NVAPI PCI match",
                        format!(
                            "ambiguous: GPU {}: complete unique PCI/BDF match not proven",
                            gpu.index
                        ),
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
    let additional_start = gpus
        .iter()
        .map(|gpu| (gpu.index as usize).saturating_add(1))
        .max()
        .unwrap_or(0);
    for (position, adapter) in adapters.iter().enumerate() {
        if position > 0 || !gpus.is_empty() {
            out.blank();
        }
        // C# parity: Hardware/GpuInfo.cs:103-120. WMI order, tree, and blank lines.
        out.text(&format!(
            "GPU {}",
            additional_start.saturating_add(position)
        ))
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

    fn test_gpu(index: u32, bus: u32) -> nvidia::Gpu {
        nvidia::Gpu {
            index,
            name: "NVIDIA Test".to_owned(),
            uuid_suffix: Some(format!(": GPU-9e521d74-03ba-4c68-a27f-81d639b504c{index}")),
            pci_bus: Some(bus),
            pci_address: Some(nvidia::PciAddress {
                bus,
                device: 0,
                function: 0,
            }),
            serial: Some(format!("03248271963{index}")),
            pdi: Some(format!("08F47A2196BC3D5{index}")),
            board_part: Some("900-1G141-2530-000".to_owned()),
            vbios: Some("94.04.3A.00.71".to_owned()),
        }
    }

    fn test_adapter(gpu: &nvidia::Gpu) -> Adapter {
        Adapter {
            name: "NVIDIA Test".to_owned(),
            pnp_id: format!("PCI\\VEN_10DE&DEV_2484\\4&73A5B19C&0&00{}", gpu.index),
            hardware_id: None,
            pci_address: gpu.pci_address,
        }
    }

    fn test_board(bus: u32, second: bool) -> nvidia::Board {
        nvidia::Board {
            pci_bus: bus,
            bytes: if second {
                *b"042571983612\0\0\0\0"
            } else {
                *b"032482719635\0\0\0\0"
            },
        }
    }

    #[test]
    fn gpu_multi_details_keep_other_fields_on_board_failure() {
        let gpus = [test_gpu(0, 1), test_gpu(1, 3)];
        let adapters = gpus.iter().map(test_adapter).collect::<Vec<_>>();
        let mut out = Out::new();
        let boards = nvidia::Capture {
            items: vec![
                test_board(3, true),
                nvidia::Board {
                    pci_bus: 1,
                    bytes: [0; 16],
                },
            ],
            failures: vec![(
                "NVAPI board",
                win::Error::msg("NvAPI_GPU_GetBoardInfo", "unsupported: fabricated failure"),
            )],
        };
        render(&mut out, &gpus, Some(&boards), &[], &adapters);
        let section = out.finish();
        let expected_prefix = "GPU 0\r\n└── NVIDIA Test\r\n    └── UUID: GPU-9e521d74-03ba-4c68-a27f-81d639b504c0\r\nGPU 1\r\n└── NVIDIA Test\r\n    └── UUID: GPU-9e521d74-03ba-4c68-a27f-81d639b504c1\r\n\r\n";
        assert!(section.body.starts_with(expected_prefix));
        assert!(!section.body.contains("GPU 0 Board Serial Number"));
        assert!(
            section
                .body
                .contains("GPU 1 Board Serial Number: 042571983612")
        );
        assert!(section.body.contains("GPU 0 Serial Number: 032482719630\r\nGPU 0 PDI: 08F47A2196BC3D50\r\nGPU 0 Board Part Number: 900-1G141-2530-000\r\nGPU 0 VBIOS Version: 94.04.3A.00.71"));
        assert!(section.body.contains("GPU 1 Serial Number: 032482719631"));
        assert!(section.failures.iter().any(|f| f.contains("unsupported")));
        let masked = report::masked(&section);
        assert!(!masked.body.contains("032482719630"));
        assert!(masked.body.contains("94.04.3A.00.71"));
    }

    #[test]
    fn gpu_board_join_rejects_whole_list_ambiguity_without_losing_identities() {
        let gpus = [test_gpu(0, 1), test_gpu(1, 3), test_gpu(2, 4)];
        let adapters = gpus.iter().map(test_adapter).collect::<Vec<_>>();
        let mut baseline = Out::new();
        render(&mut baseline, &gpus, None, &[], &adapters);
        let baseline = baseline.finish().body;
        for items in [
            vec![test_board(1, false), test_board(3, true)], // Count mismatch.
            vec![
                test_board(1, false),
                test_board(3, true),
                test_board(3, false),
            ], // Duplicate elsewhere.
            vec![
                test_board(1, false),
                test_board(3, true),
                test_board(4, false),
            ], // Duplicate board serial.
        ] {
            let boards = nvidia::Capture {
                items,
                failures: Vec::new(),
            };
            let mut out = Out::new();
            render(&mut out, &gpus, Some(&boards), &[], &adapters);
            let section = out.finish();
            if boards
                .items
                .iter()
                .filter(|board| board.pci_bus == 3)
                .count()
                > 1
                || boards.items.len() != gpus.len()
            {
                assert_eq!(section.body, baseline);
            } else {
                assert!(!section.body.contains("032482719635"));
                assert!(
                    section
                        .body
                        .contains("GPU 1 Board Serial Number: 042571983612")
                );
            }
            assert!(!section.failures.is_empty());
            assert!(section.failures.iter().all(|f| !f.contains("032482719635")));
        }
        let boards = nvidia::Capture {
            items: vec![
                test_board(1, false),
                test_board(3, true),
                nvidia::Board {
                    pci_bus: 4,
                    bytes: *b"053681294723\0\0\0\0",
                },
            ],
            failures: Vec::new(),
        };
        let mut wrong = adapters.clone();
        wrong[1].pci_address.as_mut().expect("address").device = 1;
        for rows in [&[][..], &wrong[..], &adapters[..2]] {
            let mut out = Out::new();
            render(&mut out, &gpus, Some(&boards), &[], rows);
            let section = out.finish();
            assert_eq!(section.body, baseline);
            assert!(section.failures.iter().any(|f| f.contains("ambiguous")));
        }
        let mut missing = [test_gpu(0, 1)];
        missing[0].pci_bus = None;
        let mut out = Out::new();
        render(
            &mut out,
            &missing,
            Some(&nvidia::Capture {
                items: vec![test_board(1, false)],
                failures: Vec::new(),
            }),
            &[],
            &[],
        );
        let section = out.finish();
        assert!(!section.body.contains("Board Serial Number"));
        assert!(section.body.contains("Serial Number: 032482719630"));
        assert!(!section.failures.is_empty());
    }

    #[test]
    fn gpu_mixed_and_no_nvidia_inventory_keeps_unproven_wmi_rows() {
        let gpus = [test_gpu(0, 1), test_gpu(1, 3)];
        let amd = Adapter {
            name: "AMD Radeon RX 7800 XT".to_owned(),
            pnp_id: "PCI\\VEN_1002&DEV_747E\\4&2948F31A&0&0008".to_owned(),
            hardware_id: None,
            pci_address: None,
        };
        let mut adapters = vec![test_adapter(&gpus[0]), test_adapter(&gpus[1]), amd.clone()];
        let mut out = Out::new();
        retain_additional(&mut out, &mut adapters, &gpus);
        assert_eq!(adapters.len(), 1);
        assert_eq!(adapters[0].name, amd.name);
        assert!(out.finish().failures.is_empty());
        let mut adapters = vec![test_adapter(&gpus[0]), test_adapter(&gpus[0]), amd.clone()];
        let mut out = Out::new();
        retain_additional(&mut out, &mut adapters, &gpus);
        assert_eq!(adapters.len(), 3);
        assert!(
            out.finish()
                .failures
                .iter()
                .any(|f| f.contains("ambiguous"))
        );
        let mut unknown = test_adapter(&gpus[0]);
        unknown.pci_address = None;
        let mut adapters = vec![unknown, amd.clone()];
        let mut out = Out::new();
        retain_additional(&mut out, &mut adapters, &gpus);
        assert_eq!(adapters.len(), 2);
        assert!(
            out.finish()
                .failures
                .iter()
                .any(|f| f.contains("implausible"))
        );
        let mut out = Out::new();
        render(&mut out, &[], None, std::slice::from_ref(&amd), &[]);
        assert_eq!(
            out.finish().body,
            format!("GPU 0\r\n└── {}\r\n    └── {}", amd.name, amd.pnp_id)
        );
    }

    #[test]
    fn gpu_smi_dedup_and_timeout_share_one_budget() {
        use std::{path::Path, time::Duration};
        let mut seen = std::collections::HashSet::new();
        assert_eq!(
            smi_budget(
                Path::new("C:\\NVIDIA\\nvidia-smi.exe"),
                &mut seen,
                Duration::from_secs(4)
            )
            .expect("first attempt"),
            Duration::from_secs(11)
        );
        let duplicate = smi_budget(
            Path::new("c:\\nvidia\\NVIDIA-SMI.EXE"),
            &mut seen,
            Duration::from_secs(5),
        )
        .expect_err("dedup");
        assert!(duplicate.detail.contains("already attempted"));
        assert_eq!(
            smi_budget(
                Path::new("C:\\Windows\\System32\\nvidia-smi.exe"),
                &mut seen,
                Duration::from_secs(14)
            )
            .expect("second unique"),
            Duration::from_secs(1)
        );
        let timeout = smi_budget(
            Path::new("C:\\NVSMI\\nvidia-smi.exe"),
            &mut seen,
            Duration::from_secs(16),
        )
        .expect_err("shared timeout");
        let mut out = Out::new();
        record_failures(
            &mut out,
            &[("nvidia-smi dedup", duplicate), ("nvidia-smi", timeout)],
        );
        let section = out.finish();
        assert!(section.body.is_empty());
        assert!(section.failures.iter().any(|f| f.contains("timeout")));
    }

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
        let section = out.finish();
        assert_eq!(section.body, "GPU 0\r\n└── Unknown\r\n    └── Unknown");
        assert_eq!(section.ids, ["Unknown"]);
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
                bytes: *b"032482719635\0\0\0\0",
            },
            nvidia::Board {
                pci_bus: 2,
                bytes: *b"042571983612\0\0\0\0",
            },
        ];
        assert_eq!(
            matched_board(&gpus[0], &gpus, &boards).map(|b| b.bytes),
            Some(*b"042571983612\0\0\0\0")
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
            "GPU 0\r\n└── NVIDIA\r\nGPU 1\r\n└── NVIDIA\r\n\r\nGPU 0 Board Serial Number: 042571983612\r\nGPU 1 Board Serial Number: 032482719635"
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
