//! Provider order and shared per-collection context.

pub mod arp;
pub mod battery;
pub mod bios;
pub mod bluetooth;
pub mod chassis;
pub mod cpu;
pub mod disk;
pub mod gpu;
pub mod monitor;
pub mod motherboard;
pub mod network;
pub mod ram;
pub mod system;
pub mod tpm;
pub mod usb;

mod collection;
#[cfg(test)]
mod tests;

pub use collection::{collect_all, collect_all_with_options, collect_provider};

use crate::{
    report::{self, Out, Section},
    win::{self, firmware::Smbios},
};
use std::{
    collections::{HashMap, HashSet},
    sync::OnceLock,
};

pub struct Provider {
    pub title: &'static str,
    pub collect: fn(&Ctx, &mut Out) -> Result<(), win::Error>,
}

// C# parity: HardwareInfoManager.cs:23-38.
pub static PROVIDERS: [Provider; 15] = [
    Provider {
        title: "DISK DRIVES",
        collect: disk::collect,
    },
    Provider {
        title: "MOTHERBOARD",
        collect: motherboard::collect,
    },
    Provider {
        title: "CHASSIS",
        collect: chassis::collect,
    },
    Provider {
        title: "(SM)BIOS",
        collect: bios::collect,
    },
    Provider {
        title: "SYSTEM INFORMATION",
        collect: system::collect,
    },
    Provider {
        title: "RAM MODULES",
        collect: ram::collect,
    },
    Provider {
        title: "CPU",
        collect: cpu::collect,
    },
    Provider {
        title: "TPM MODULES",
        collect: tpm::collect,
    },
    Provider {
        title: "USB DEVICES",
        collect: usb::collect,
    },
    Provider {
        title: "GPU INFO",
        collect: gpu::collect,
    },
    Provider {
        title: "MONITOR INFORMATION",
        collect: monitor::collect,
    },
    Provider {
        title: "NETWORK ADAPTERS (NIC's)",
        collect: network::collect,
    },
    Provider {
        title: "BLUETOOTH ADAPTERS",
        collect: bluetooth::collect,
    },
    Provider {
        title: "BATTERY",
        collect: battery::collect,
    },
    Provider {
        title: "ARP INFO/CACHE",
        collect: arp::collect,
    },
];

/// Immutable preferences for one scan. Baseline and timing callers use the default (Off).
#[derive(Clone, Copy, Debug, Default)]
pub struct CollectionOptions {
    pub deep_hardware_information: bool,
}

enum DeepInformationStatus {
    Unavailable,
    Available {
        partial: bool,
    },
    Unknown,
    Blocked {
        names: String,
        associated_games: Vec<String>,
    },
}

#[derive(Default)]
pub struct Ctx {
    options: CollectionOptions,
    deep_status: OnceLock<DeepInformationStatus>,
    deep_spd: OnceLock<win::pawnio::Snapshot>,
    anticheat: OnceLock<win::anticheat::Detection>,
    hardware_ids: OnceLock<win::Result<HashMap<String, String>>>,
    smbios: OnceLock<win::Result<Smbios>>,
    present_ids: OnceLock<win::Result<HashSet<String>>>,
}

impl Ctx {
    /// Creates caches shared only within this collection.
    pub fn new() -> Self {
        Self::default()
    }
    /// Snapshots preferences without opening a source or consulting a mutable setting.
    pub fn with_options(options: CollectionOptions) -> Self {
        Self {
            options,
            ..Self::default()
        }
    }

    fn deep_information_status(&self) -> Option<&DeepInformationStatus> {
        if !self.options.deep_hardware_information {
            return None;
        }
        Some(self.deep_status.get_or_init(|| {
            use win::anticheat::{Gate, GateDecision};
            let snapshot = self
                .deep_spd
                .get()
                .and_then(|spd| spd.gate.as_ref())
                .unwrap_or_else(|| self.anticheat());
            match snapshot.gate_decision() {
                GateDecision::Clear => {
                    self.deep_spd
                        .get()
                        .map_or(DeepInformationStatus::Unavailable, |spd| {
                            if spd.modules.is_empty() {
                                DeepInformationStatus::Unavailable
                            } else {
                                DeepInformationStatus::Available {
                                    partial: !spd.failures.is_empty()
                                        || spd.modules.iter().any(|module| {
                                            !module.manufacturing.failures.is_empty()
                                        }),
                                }
                            }
                        })
                }
                GateDecision::Unknown => DeepInformationStatus::Unknown,
                GateDecision::Blocked => {
                    let mut names = Vec::new();
                    let mut associated_games = Vec::new();
                    for entry in snapshot
                        .detected
                        .iter()
                        .filter(|entry| entry.gate != Gate::Dont)
                    {
                        // Only an exact title/executable-stem match identifies an active game.
                        // Shared launchers, service names and sample titles alone do not.
                        let games: Vec<_> = entry
                            .games_sample
                            .iter()
                            .filter(|game| {
                                let normalized = normalize_game_name(game);
                                !normalized.is_empty()
                                    && entry.evidence.iter().any(|hit| {
                                        hit.layer == win::anticheat::Layer::Process
                                            && hit.matched_name.rsplit_once('.').is_some_and(
                                                |(stem, extension)| {
                                                    extension.eq_ignore_ascii_case("exe")
                                                        && normalize_game_name(stem) == normalized
                                                },
                                            )
                                    })
                            })
                            .map(String::as_str)
                            .collect();
                        if games.is_empty() {
                            names.push(entry.name.clone());
                            if !entry.games_sample.is_empty() {
                                associated_games.push(format!(
                                    "Associated games ({}; active title unknown): {}",
                                    entry.name,
                                    entry.games_sample.join(", ")
                                ));
                            }
                        } else {
                            names.push(format!("{} / {}", games.join(", "), entry.name));
                        }
                    }
                    DeepInformationStatus::Blocked {
                        names: names.join(", "),
                        associated_games,
                    }
                }
            }
        }))
    }
    /// Only a selected RAM collection requests this optional source, once per scan.
    /// OFF/blocked/unknown never reach the PawnIO factory, including package access.
    fn prepare_deep_information(&self) {
        if self.options.deep_hardware_information
            && self.anticheat().gate_decision() == win::anticheat::GateDecision::Clear
        {
            self.deep_spd.get_or_init(|| {
                let result = win::pawnio::collect(self.anticheat());
                if let Some(gate) = &result.gate {
                    for failure in &gate.failed_layers {
                        win::record(anticheat_error(failure));
                    }
                }
                result
            });
        }
    }
    /// Returns this collection's bounded advisory snapshot, including unknown results.
    pub fn anticheat(&self) -> &win::anticheat::Detection {
        self.anticheat.get_or_init(|| {
            let snapshot = win::anticheat::detect();
            for failure in &snapshot.failed_layers {
                // Keep diagnostics even when SYSTEM INFORMATION is filtered out.
                win::record(anticheat_error(failure));
            }
            snapshot
        })
    }
    /// Returns a cached hardware ID using uppercase instance-ID normalization.
    pub fn hardware_id(&self, instance_id: &str) -> Option<&str> {
        // Keep a failed snapshot in OnceLock; hardware_ids() exposes it for diagnostics.
        self.hardware_ids()
            .ok()?
            .get(&instance_id.to_uppercase())
            .map(String::as_str)
    }
    /// Returns the cached SMBIOS table when that source succeeded.
    pub fn smbios(&self) -> Option<&Smbios> {
        // Keep a failed snapshot in OnceLock; smbios_result() exposes it for diagnostics.
        self.smbios_result().ok()
    }
    /// Returns the cached map or its error so providers can record a failed source.
    pub fn hardware_ids(&self) -> win::Result<&HashMap<String, String>> {
        self.hardware_ids
            .get_or_init(win::setupapi::hardware_id_map)
            .as_ref()
            .map_err(Clone::clone)
    }
    /// Returns the cached firmware table or its error for fallback diagnostics.
    pub fn smbios_result(&self) -> win::Result<&Smbios> {
        self.smbios
            .get_or_init(win::firmware::smbios)
            .as_ref()
            .map_err(Clone::clone)
    }
    /// Returns normalized present IDs or their error; unknown presence stays unknown.
    pub fn present_instance_ids(&self) -> win::Result<&HashSet<String>> {
        self.present_ids
            .get_or_init(win::setupapi::present_instance_ids)
            .as_ref()
            .map_err(Clone::clone)
    }
}

fn normalize_game_name(value: &str) -> String {
    value
        .chars()
        .filter(char::is_ascii_alphanumeric)
        .flat_map(char::to_lowercase)
        .collect()
}

fn anticheat_error(failure: &win::anticheat::FailedLayer) -> win::Error {
    win::Error {
        op: failure.error.op,
        code: failure.error.code,
        // Enumeration strings are not diagnostic payloads; retain the API and code.
        detail: format!("anti-cheat {:?} layer unavailable", failure.layer),
    }
}

/// Formats a raw report with the legacy header and ordered section headings.
pub fn full_report(sections: &[Section]) -> String {
    // C# parity: HardwareInfoManager.cs:51-52,99-103; SectionedViewForm.cs:833.
    let mut body = report::format_header("Comprehensive HWID Checker");
    for section in sections {
        body.push_str(&report::format_section(section.title, &section.body));
    }
    body
}

/// Tries sources in order, recording failures and returning the first success or last error.
#[allow(clippy::type_complexity)] // PLAN 1.6a freezes this explicit callback signature.
pub fn first_ok<T>(
    out: &mut Out,
    op: &'static str,
    sources: &[(&'static str, &dyn Fn() -> Result<T, win::Error>)],
) -> Result<T, win::Error> {
    let mut last_error = win::Error::msg(op, "no sources provided");
    for &(source, collect) in sources {
        match collect() {
            Ok(value) => {
                out.source(source);
                return Ok(value);
            }
            Err(error) => {
                out.fallback_failed(source, &error);
                last_error = error;
            }
        }
    }
    Err(last_error)
}
