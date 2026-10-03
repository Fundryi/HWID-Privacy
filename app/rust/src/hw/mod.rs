//! Provider order and shared per-collection context.

pub mod arp;
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

use crate::{
    report::{self, Out, Section},
    win::{self, firmware::Smbios},
};
use std::{
    collections::{HashMap, HashSet},
    panic::{AssertUnwindSafe, catch_unwind},
    sync::OnceLock,
    time::Instant,
};

pub struct Provider {
    pub title: &'static str,
    pub collect: fn(&Ctx, &mut Out) -> Result<(), win::Error>,
}

// C# parity: HardwareInfoManager.cs:23-38.
pub static PROVIDERS: [Provider; 14] = [
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
        title: "ARP INFO/CACHE",
        collect: arp::collect,
    },
];

#[derive(Default)]
pub struct Ctx {
    hardware_ids: OnceLock<win::Result<HashMap<String, String>>>,
    smbios: OnceLock<win::Result<Smbios>>,
    present_ids: OnceLock<win::Result<HashSet<String>>>,
}

impl Ctx {
    /// Creates caches shared only within this collection.
    pub fn new() -> Self {
        Self::default()
    }
    /// Returns a cached hardware ID using uppercase instance-ID normalization.
    pub fn hardware_id(&self, instance_id: &str) -> Option<&str> {
        self.hardware_ids()
            .ok()?
            .get(&instance_id.to_uppercase())
            .map(String::as_str)
    }
    /// Returns the cached SMBIOS table when that source succeeded.
    pub fn smbios(&self) -> Option<&Smbios> {
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

/// Collects selected provider stubs with readable error sections and diagnostic timings.
pub fn collect_all(only: Option<&str>, on_done: &(dyn Fn(usize, &Section) + Sync)) -> Vec<Section> {
    let ctx = Ctx::new();
    PROVIDERS
        .iter()
        .enumerate()
        .filter(|(_, p)| only.is_none_or(|title| report::eq_ignore_case(title, p.title)))
        .map(|(index, provider)| {
            let section = collect_provider(provider, &ctx);
            on_done(index, &section);
            section
        })
        .collect()
}

/// Runs one provider for diagnostic timing, preserving errors and caught panics.
pub fn collect_provider(provider: &Provider, ctx: &Ctx) -> Section {
    let start = Instant::now();
    let mut out = Out::new();
    match catch_unwind(AssertUnwindSafe(|| (provider.collect)(ctx, &mut out))) {
        Ok(Ok(())) => {}
        Ok(Err(error)) => {
            out.fallback_failed(provider.title, &error);
            out.text(&format!(
                "Error retrieving {} information: {error}",
                provider.title
            ));
        }
        Err(_) => {
            out.text(&format!(
                "Error retrieving {} information: provider panicked",
                provider.title
            ));
        }
    }
    let mut section = out.finish();
    section.title = provider.title;
    section.elapsed_ms = start.elapsed().as_millis();
    section
}

/// Formats a raw report with the legacy header and ordered section headings.
pub fn full_report(sections: &[Section]) -> String {
    let mut body = report::format_header("Comprehensive HWID Checker");
    for section in sections {
        body.push_str(&report::format_section(section.title, &section.body));
    }
    body
}

/// Frozen fallback-chain contract; implementation belongs to the hw-context fill-in.
#[allow(clippy::type_complexity)] // PLAN 1.6a freezes this explicit callback signature.
pub fn first_ok<T>(
    _out: &mut Out,
    op: &'static str,
    _sources: &[(&'static str, &dyn Fn() -> Result<T, win::Error>)],
) -> Result<T, win::Error> {
    Err(win::Error::msg(op, "fallback chain not ported yet"))
}
