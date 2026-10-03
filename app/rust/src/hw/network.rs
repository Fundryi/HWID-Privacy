//! WMI NIC enumeration, SetupAPI enrichment, and the approved permanent-MAC policy.

use crate::{
    hw::Ctx,
    report::{self, Out},
    win::{
        self,
        iphlp::{self, Interface},
        registry, wmi,
    },
};
use windows::core::GUID;

const NET_CLASS: &str =
    r"SYSTEM\CurrentControlSet\Control\Class\{4d36e972-e325-11ce-bfc1-08002be10318}";

#[cfg_attr(test, derive(serde::Deserialize))]
struct Adapter {
    name: String,
    product_name: String,
    device_id: String,
    adapter_type: String,
    pnp_device_id: String,
    guid: Option<String>,
    mac: Option<String>,
    physical: Option<bool>,
}

impl Adapter {
    fn from_row(row: &wmi::Row) -> Self {
        Self {
            name: row.str("Name").unwrap_or_default(),
            product_name: row.str("ProductName").unwrap_or_default(),
            device_id: row.str("DeviceID").unwrap_or_default(),
            adapter_type: row.str("AdapterType").unwrap_or_default(),
            pnp_device_id: row.str("PNPDeviceID").unwrap_or_default(),
            guid: row.str("GUID"),
            mac: row.str("MACAddress"),
            physical: row.bool("PhysicalAdapter"),
        }
    }

    fn is_real(&self) -> bool {
        // C# parity: NetworkInfo.cs:40-80,94. Empty MACs are accepted; null is not.
        if self.mac.is_none() || self.physical == Some(false) {
            return false;
        }
        let pnp = self.pnp_device_id.to_uppercase();
        let mellanox = pnp.starts_with("MLX4\\") || pnp.starts_with("MLX5\\");
        let bus = mellanox
            || pnp.starts_with("PCI\\")
            || pnp.starts_with("USB\\")
            || pnp.contains("PCI_")
            || pnp.contains("USB_");
        let name = self.name.to_uppercase();
        let product = self.product_name.to_uppercase();
        let virtual_adapter = [
            "VIRTUAL",
            "VPN",
            "TAP",
            "TUN",
            "TUNNEL",
            "VMWARE",
            "HYPER-V",
            "VIRTUALBOX",
            "CISCO",
            "CHECKPOINT",
            "FORTINET",
            "JUNIPER",
            "CITRIX",
            "SOFTETHER",
            "OPENVPN",
            "WIREGUARD",
            "GHOST",
            "HAMACHI",
            "NDIS",
            "BRIDGE",
            "LOOPBACK",
        ]
        .iter()
        .any(|keyword| name.contains(keyword) || product.contains(keyword));
        let kind = self.adapter_type.to_uppercase();
        let physical_type = ["ETHERNET", "802.3", "WIRELESS", "WI-FI", "WIFI", "802.11"]
            .iter()
            .any(|keyword| kind.contains(keyword));
        bus && !virtual_adapter && (physical_type || mellanox)
    }
}

/// Collects this hardware section through the shared output builder.
pub fn collect(ctx: &Ctx, out: &mut Out) -> Result<(), win::Error> {
    out.source("WMI");
    // C# parity: NetworkInfo.cs:86,92-100. Keep WMI enumeration order and DeviceID semantics.
    let adapters: Vec<_> = wmi::query(wmi::Namespace::Cimv2, "SELECT * FROM Win32_NetworkAdapter")?
        .iter()
        .map(Adapter::from_row)
        .filter(Adapter::is_real)
        .collect();
    if adapters.is_empty() {
        return Ok(());
    }
    let hardware_ids = match ctx.hardware_ids() {
        Ok(map) => Some(map),
        Err(error) => {
            out.fallback_failed("SetupAPI", &error);
            None
        }
    };
    let interfaces = match iphlp::interface_table() {
        Ok(rows) => rows,
        Err(error) => {
            out.fallback_failed("native", &error);
            Vec::new()
        }
    };
    let mut sources = vec!["WMI"];
    if hardware_ids.is_some() {
        sources.push("SetupAPI");
    }
    if !interfaces.is_empty() {
        sources.push("native");
    }
    for (index, adapter) in adapters.iter().enumerate() {
        let mut override_error = None;
        let permanent = permanent_mac(adapter.guid.as_deref(), &interfaces, out);
        let overridden = if permanent.is_none() {
            sources.push("registry");
            match registry_mac_override(&adapter.pnp_device_id) {
                Ok(overridden) => overridden,
                Err(error) => {
                    out.fallback_failed("registry", &error);
                    override_error = Some(error);
                    false
                }
            }
        } else {
            false
        };
        let hardware_id = hardware_ids
            .and_then(|map| map.get(&adapter.pnp_device_id.to_uppercase()))
            .map(String::as_str);
        append_adapter(adapter, hardware_id, permanent.as_deref(), overridden, out);
        if let Some(error) = override_error {
            out.text(&format!("MAC override lookup: {error}"));
        }
        // C# parity: NetworkInfo.cs:139-143. No trailing item separator.
        if index + 1 < adapters.len() {
            out.separator();
        }
    }
    if let Err(error) = ctx.hardware_ids() {
        // No alternate source can supply the missing first SetupAPI hardware ID (AD-03).
        out.text(&format!("Hardware ID lookup: {error}"));
    }
    sources.dedup();
    out.source(&sources.join(" + "));
    Ok(())
}

fn permanent_mac(guid: Option<&str>, rows: &[Interface], out: &mut Out) -> Option<String> {
    let guid = guid?.trim_matches(['{', '}']);
    let guid = match GUID::try_from(guid) {
        Ok(guid) => guid,
        Err(error) => {
            out.fallback_failed(
                "WMI GUID",
                &win::Error::from_win("NIC interface GUID", error),
            );
            return None;
        }
    };
    let mut matches = rows.iter().filter(|row| row.guid == guid);
    let Some(row) = matches.next() else {
        out.fallback_failed(
            "native",
            &win::Error::msg("GetIfTable2", "no interface row matches WMI GUID"),
        );
        return None;
    };
    if matches.next().is_some() {
        out.fallback_failed(
            "native",
            &win::Error::msg("GetIfTable2", "ambiguous interface GUID"),
        );
        return None;
    }
    let length = row.physical_address_length as usize;
    let Some(address) = row.permanent_physical_address.get(..length) else {
        out.fallback_failed(
            "native",
            &win::Error::msg("GetIfTable2", "MAC length exceeds 32 bytes"),
        );
        return None;
    };
    if address.is_empty() || address.iter().all(|byte| *byte == 0) {
        return None;
    }
    Some(
        address
            .iter()
            .map(|byte| format!("{byte:02X}"))
            .collect::<Vec<_>>()
            .join(":"),
    )
}

fn append_adapter(
    adapter: &Adapter,
    hardware_id: Option<&str>,
    permanent: Option<&str>,
    registry_overridden: bool,
    out: &mut Out,
) {
    // C# parity: NetworkInfo.cs:103-118,134-136. Keep the same fields and their order.
    out.info("Name", &adapter.name)
        .info("Product Name", &adapter.product_name)
        .info("Device ID", &adapter.device_id)
        .info(
            "Adapter Type",
            &simplify_adapter_type(&adapter.adapter_type),
        );
    if let Some(hardware_id) = hardware_id {
        out.id("Hardware ID", hardware_id);
    }
    let current = adapter.mac.as_deref().unwrap_or_default();
    let overridden = match permanent {
        Some(permanent) => !current.is_empty() && !report::eq_ignore_case(current, permanent),
        None => registry_overridden,
    };
    // AD-20: effective comparison takes precedence over a stale registry configuration.
    out.id(
        if overridden {
            "MAC Address (Overridden)"
        } else {
            "MAC Address"
        },
        current,
    );
    if let Some(permanent) = permanent {
        out.id("Permanent MAC", permanent);
    } else {
        out.info("Permanent MAC", "Unavailable");
    }
}

fn simplify_adapter_type(kind: &str) -> String {
    // C# parity: NetworkInfo.cs:20-37. Do not trim the original WMI value.
    if kind.is_empty() {
        return "Unknown".into();
    }
    let kind = kind.to_uppercase();
    if ["802.11", "WIRELESS", "WI-FI", "WIFI"]
        .iter()
        .any(|part| kind.contains(part))
    {
        "WiFi".into()
    } else if kind.contains("802.3") || kind.contains("ETHERNET") {
        "Ethernet".into()
    } else if kind.contains("BLUETOOTH") {
        "Bluetooth".into()
    } else {
        kind
    }
}

fn optional_registry_string(path: &str, name: &str) -> win::Result<Option<String>> {
    match registry::read_string(path, name) {
        Ok(value) => Ok(Some(value)),
        // C# parity: NetworkInfo.cs:170-175,185. An absent key/value is no configuration.
        Err(error) if matches!(error.code, 2 | 3) => Ok(None),
        Err(error) => Err(error),
    }
}

fn registry_mac_override(pnp_device_id: &str) -> win::Result<bool> {
    if pnp_device_id.is_empty() {
        return Ok(false);
    }
    let subkeys = match registry::subkeys(NET_CLASS) {
        Ok(subkeys) => subkeys,
        Err(error) if matches!(error.code, 2 | 3) => return Ok(false),
        Err(error) => return Err(error),
    };
    for name in subkeys {
        // C# parity: NetworkInfo.cs:165-168. Ignore nonnumeric class subkeys.
        if name.trim().parse::<i32>().is_err() {
            continue;
        }
        let path = format!(r"{NET_CLASS}\{name}");
        let instance = optional_registry_string(&path, "DeviceInstanceID")?;
        let matching = optional_registry_string(&path, "MatchingDeviceId")?;
        // C# parity: NetworkInfo.cs:177-193. Retain prefix matches and stale-entry traversal.
        let matches = instance
            .as_deref()
            .is_some_and(|id| !id.is_empty() && report::eq_ignore_case(id, pnp_device_id))
            || matching.as_deref().is_some_and(|id| {
                !id.is_empty()
                    && pnp_device_id
                        .get(..id.len())
                        .is_some_and(|prefix| report::eq_ignore_case(prefix, id))
            });
        if matches {
            let value = optional_registry_string(&path, "NetworkAddress")?;
            // C# parity: NetworkInfo.cs:185-188,216-223. Only the formatted value's presence
            // matters here; separator-only values are empty, but nonhex configurations count.
            if value.is_some_and(|value| value.chars().any(|ch| !matches!(ch, '-' | ':' | '.'))) {
                return Ok(true);
            }
        }
    }
    Ok(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(serde::Deserialize)]
    struct Case {
        adapter: Adapter,
        hardware_id: Option<String>,
        interface_guid: Option<String>,
        permanent: Vec<u8>,
        length: u32,
        registry_overridden: bool,
        expected: String,
    }

    #[test]
    fn network_fixture_preserves_fields_filter_and_ad20_mac_lines() {
        let cases: Vec<Case> = serde_json::from_str(include_str!(
            "../../tests/fixtures/wp-09/network-format.json"
        ))
        .expect("fabricated network fixture");
        for case in cases {
            let mut out = Out::new();
            let mut interfaces = Vec::new();
            if let Some(guid) = case.interface_guid {
                let mut row = Interface {
                    guid: GUID::try_from(guid.as_str()).expect("fixture GUID"),
                    physical_address_length: case.length,
                    ..Default::default()
                };
                row.permanent_physical_address[..case.permanent.len()]
                    .copy_from_slice(&case.permanent);
                interfaces.push(row);
            }
            if case.adapter.is_real() {
                let permanent = permanent_mac(case.adapter.guid.as_deref(), &interfaces, &mut out);
                append_adapter(
                    &case.adapter,
                    case.hardware_id.as_deref(),
                    permanent.as_deref(),
                    case.registry_overridden,
                    &mut out,
                );
            }
            let section = out.finish();
            assert_eq!(section.body, case.expected, "{}", case.adapter.name);
            assert!(!section.ids.iter().any(|value| value == "Unavailable"));
            if case.length > 32 {
                assert!(
                    section
                        .failures
                        .iter()
                        .any(|failure| failure.contains("MAC length exceeds 32 bytes"))
                );
            }
        }
    }

    #[test]
    fn network_permanent_mac_rejects_ambiguous_guids_and_zero_addresses() {
        let guid = GUID::from_u128(0x6eba2c67_791a_4cbd_bfaa_4b9bbfc438ad);
        let mut row = Interface {
            guid,
            physical_address_length: 6,
            ..Default::default()
        };
        row.permanent_physical_address[..6].copy_from_slice(&[0x3c, 0xfd, 0xfe, 0x64, 0x19, 0x82]);
        let id = "{6eba2c67-791a-4cbd-bfaa-4b9bbfc438ad}";
        let mut out = Out::new();
        assert!(permanent_mac(Some(id), &[], &mut out).is_none());
        let missing = out.finish();
        assert!(missing.body.is_empty());
        assert!(missing.failures[0].contains("no interface row matches WMI GUID"));
        let mut out = Out::new();
        assert!(permanent_mac(Some(id), &[row.clone(), row.clone()], &mut out).is_none());
        assert!(out.finish().failures[0].contains("ambiguous interface GUID"));
        row.permanent_physical_address = [0; 32];
        assert!(permanent_mac(Some(id), &[row], &mut Out::new()).is_none());
    }

    #[test]
    #[ignore = "reads real identifiers; redirect stdout into the private golden/wp-09 directory"]
    fn wp09_capture_network_and_arp() {
        if win::security::is_admin() {
            return;
        }
        let ctx = Ctx::new();
        let sections: Vec<_> = crate::hw::PROVIDERS
            .iter()
            .filter(|provider| {
                matches!(
                    provider.title,
                    "NETWORK ADAPTERS (NIC's)" | "ARP INFO/CACHE"
                )
            })
            .map(|provider| crate::hw::collect_provider(provider, &ctx))
            .collect();
        println!(
            "WP09_REPORT_BEGIN\n{}WP09_REPORT_END",
            crate::hw::full_report(&sections)
        );
        println!("WP09_DIAGNOSTICS_BEGIN");
        for section in sections {
            println!(
                "{}: {} ms; source={}",
                section.title, section.elapsed_ms, section.source
            );
            for failure in section.failures {
                println!("Failure: {failure}");
            }
        }
        println!("WP09_DIAGNOSTICS_END");
    }
}
