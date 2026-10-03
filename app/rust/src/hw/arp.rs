//! Neighbor-cache formatting and the bounded legacy arp.exe fallback.

use crate::{
    hw::{self, Ctx},
    report::Out,
    win::{self, process},
};
use std::{
    collections::{BTreeMap, HashMap},
    net::{IpAddr, Ipv4Addr, Ipv6Addr},
    time::Duration,
};
use windows::Win32::Networking::WinSock::{NL_NEIGHBOR_STATE, NlnsIncomplete};

const EMPTY_STATUS: &str = "No relevant dynamic ARP entries found.";

struct Neighbor {
    interface_index: u32,
    ip: IpAddr,
    physical_address: Vec<u8>,
    state: NL_NEIGHBOR_STATE,
}

enum Cache {
    Native(Vec<Neighbor>),
    ArpExe(process::Output),
}

/// Collects this hardware section through the shared output builder.
pub fn collect(_ctx: &Ctx, out: &mut Out) -> Result<(), win::Error> {
    let native = || neighbor_table().map(Cache::Native);
    let fallback = || arp_exe().map(Cache::ArpExe);
    // C# parity: ArpInfo.cs:24-40. A successful empty table must not trigger arp.exe.
    match hw::first_ok(out, "ARP", &[("native", &native), ("arp.exe", &fallback)]) {
        Ok(Cache::Native(entries)) => {
            if !entries.iter().any(relevant_neighbor) {
                format_neighbors(&entries, &HashMap::new(), out);
                return Ok(());
            }
            let names = match interface_names() {
                Ok(names) => names,
                Err(error) => {
                    out.fallback_failed("GetAdaptersAddresses", &error);
                    format_neighbors(&entries, &HashMap::new(), out);
                    out.text(&format!("Interface name lookup: {error}"));
                    return Ok(());
                }
            };
            format_neighbors(&entries, &names, out);
        }
        Ok(Cache::ArpExe(output)) => {
            if !output.stderr.trim().is_empty() {
                out.fallback_failed(
                    "arp.exe stderr",
                    &win::Error::msg("arp.exe", &output.stderr),
                );
            }
            format_arp_exe(&output.stdout, out);
        }
        Err(error) => {
            // C# parity: ArpInfo.cs:175-177. Keep the fixed text around the error (AD-01).
            out.info(
                "Error",
                &format!("Unable to retrieve ARP information: {error}"),
            );
        }
    }
    // blocked: Out has no TrimEnd operation. The final CRLF is identical in full_report
    // because format_section adds it to the trimmed C# body (ArpInfo.cs:29,36,40).
    Ok(())
}

fn neighbor_table() -> win::Result<Vec<Neighbor>> {
    // blocked: GetIpNetTable2 + FreeMibTable need an orchestrator-owned safe win/ wrapper.
    Err(win::Error::msg(
        "GetIpNetTable2",
        "blocked: safe win helper is absent from the frozen base",
    ))
}

fn interface_names() -> win::Result<HashMap<u32, String>> {
    // blocked: GetAdaptersAddresses must return owned names keyed by both indices (AD-21).
    Err(win::Error::msg(
        "GetAdaptersAddresses",
        "blocked: safe win helper is absent from the frozen base",
    ))
}

fn arp_exe() -> win::Result<process::Output> {
    // C# parity: ArpInfo.cs:124-138. Absolute System32 path and a timeout replace the bare
    // executable name and unbounded WaitForExit, without changing the successful text.
    let output = process::run(
        &process::system32("arp.exe"),
        &["-a"],
        Duration::from_secs(10),
        &process::Cancel::new(),
    )
    .map_err(|error| win::Error::msg("arp.exe", error))?;
    if output.code != 0 {
        return Err(win::Error::msg("arp.exe", output.stderr.trim()));
    }
    Ok(output)
}

fn format_neighbors(entries: &[Neighbor], names: &HashMap<u32, String>, out: &mut Out) {
    let mut groups = BTreeMap::<u32, Vec<(bool, String, String)>>::new();
    for entry in entries {
        // C# parity: IpHlpApi.cs:121-132. Only state 1 is dropped, despite the incorrect
        // C# enum name/comment. OPT-9 retains Unreachable and Permanent entries.
        let address = &entry.physical_address[..entry.physical_address.len().min(32)];
        if entry.physical_address.len() > 32 {
            out.fallback_failed(
                "native",
                &win::Error::msg("GetIpNetTable2", "MAC length capped at 32 bytes"),
            );
        }
        if !relevant_neighbor(entry) {
            continue;
        }
        // C# parity: IpHlpApi.cs:99-112. IPv6 scope IDs are deliberately not displayed.
        let ip = ip_text(entry.ip);
        let mac = address
            .iter()
            .map(|byte| format!("{byte:02X}"))
            .collect::<Vec<_>>()
            .join(":");
        groups
            .entry(entry.interface_index)
            .or_default()
            .push((entry.ip.is_ipv6(), ip, mac));
    }
    if groups.is_empty() {
        out.info("Status", EMPTY_STATUS);
        return;
    }
    // C# parity: ArpInfo.cs:49-84. Numeric interface order, then IPv4/IPv6 string order.
    let count = groups.len();
    for (ordinal, (index, mut entries)) in groups.into_iter().enumerate() {
        let name = names
            .get(&index)
            .cloned()
            .unwrap_or_else(|| format!("Interface #{index}"));
        out.text(&format!(
            "[{name}]{}",
            if is_virtual_interface(&name) {
                " (Virtual)"
            } else {
                ""
            }
        ));
        entries.sort_by(|left, right| left.0.cmp(&right.0).then(left.1.cmp(&right.1)));
        for (ipv6, ip, mac) in entries {
            out.combined(&[
                ("MAC", &mac, true),
                (if ipv6 { "IPv6" } else { "IP" }, &ip, true),
            ]);
        }
        // ArpInfo.cs:84 adds a blank after the last group, then :29 trims it away.
        if ordinal + 1 < count {
            out.blank();
        }
    }
}

fn relevant_neighbor(entry: &Neighbor) -> bool {
    // C# parity: IpHlpApi.cs:121-132. Do not discard Unreachable or Permanent states.
    let address = &entry.physical_address[..entry.physical_address.len().min(32)];
    entry.state != NlnsIncomplete
        && !address.is_empty()
        && address.iter().any(|byte| *byte != 0)
        && address[0] & 1 == 0
}

fn ip_text(ip: IpAddr) -> String {
    // C# parity: IpHlpApi.cs:105,112. .NET preserves dotted tails for IPv4-compatible
    // and 0000:5efe ISATAP addresses, where Rust's default display uses hex groups.
    if let IpAddr::V6(ipv6) = ip {
        let parts = ipv6.segments();
        let compatible = parts[..6] == [0; 6] && parts[6] != 0;
        let isatap = parts[4] == 0 && parts[5] == 0x5efe;
        if compatible || isatap {
            let bytes = ipv6.octets();
            let ipv4 = Ipv4Addr::new(bytes[12], bytes[13], bytes[14], bytes[15]);
            if compatible {
                return format!("::{ipv4}");
            }
            // Nonzero tail groups keep std's zero compression inside the six IPv6
            // groups. Replace that known suffix with .NET's dotted IPv4 tail.
            let prefix = Ipv6Addr::new(
                parts[0], parts[1], parts[2], parts[3], parts[4], parts[5], 0xabcd, 0xef01,
            );
            return format!("{}{ipv4}", prefix.to_string().trim_end_matches("abcd:ef01"));
        }
    }
    ip.to_string()
}

fn is_virtual_interface(name: &str) -> bool {
    // C# parity: ArpInfo.cs:88-99. This is deliberately narrower than the NIC filter.
    let name = name.to_uppercase();
    [
        "VETHERNET",
        "LOOPBACK",
        "WSL",
        "DOCKER",
        "HYPER-V",
        "VPN",
        "VMWARE",
        "VIRTUALBOX",
    ]
    .iter()
    .any(|keyword| name.contains(keyword))
}

fn format_arp_exe(output: &str, out: &mut Out) {
    let mut has_entries = false;
    // C# parity: ArpInfo.cs:140-167. English, case-sensitive substrings and space-only
    // splitting are intentional; the fallback does not normalize case or sort entries.
    for line in output.split(['\r', '\n']).filter(|line| !line.is_empty()) {
        if line.contains("Interface:")
            || !line.contains('-')
            || line.contains("ff-ff-ff-ff-ff-ff")
            || line.contains("01-00-5e")
            || line.ends_with("static")
            || !line.contains("dynamic")
        {
            continue;
        }
        let parts: Vec<_> = line.split(' ').filter(|part| !part.is_empty()).collect();
        if parts.len() < 3 {
            continue;
        }
        if !has_entries {
            out.text("Dynamic ARP Entries:");
            has_entries = true;
        }
        out.combined(&[
            ("MAC", &parts[1].replace('-', ":"), true),
            ("IP", parts[0], true),
        ]);
    }
    if !has_entries {
        // C# parity: ArpInfo.cs:170-173. Empty and localized output have the same status.
        out.info("Status", EMPTY_STATUS);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(serde::Deserialize)]
    struct Entry {
        interface_index: u32,
        ip: IpAddr,
        physical_address: Vec<u8>,
        state: i32,
    }

    #[derive(serde::Deserialize)]
    struct NativeFixture {
        names: HashMap<u32, String>,
        entries: Vec<Entry>,
        expected: String,
    }

    #[test]
    fn arp_native_fixture_preserves_opt9_sort_tags_and_identifier_records() {
        let fixture: NativeFixture = serde_json::from_str(include_str!(
            "../../tests/fixtures/wp-09/arp-native-format.json"
        ))
        .expect("fabricated neighbor fixture");
        let entries: Vec<_> = fixture
            .entries
            .into_iter()
            .map(|entry| Neighbor {
                interface_index: entry.interface_index,
                ip: entry.ip,
                physical_address: entry.physical_address,
                state: NL_NEIGHBOR_STATE(entry.state),
            })
            .collect();
        let mut out = Out::new();
        format_neighbors(&entries, &fixture.names, &mut out);
        let section = out.finish();
        assert_eq!(section.body, fixture.expected);
        assert_eq!(section.ids.len(), 14);
        assert!(section.failures.is_empty());
    }

    #[test]
    fn arp_native_caps_mac_at_32_bytes_and_empty_has_legacy_status() {
        let mut address = vec![0x3c, 0xfd, 0xfe, 0x64, 0x19, 0x82];
        address.extend(6..33);
        let entry = Neighbor {
            interface_index: 7,
            ip: "192.0.2.11".parse().expect("fixture IP"),
            physical_address: address,
            state: windows::Win32::Networking::WinSock::NlnsReachable,
        };
        let mut out = Out::new();
        format_neighbors(&[entry], &HashMap::new(), &mut out);
        let section = out.finish();
        assert_eq!(
            section.body,
            "[Interface #7]\r\nMAC: 3C:FD:FE:64:19:82:06:07:08:09:0A:0B:0C:0D:0E:0F:10:11:12:13:14:15:16:17:18:19:1A:1B:1C:1D:1E:1F | IP: 192.0.2.11\r\n"
        );
        assert!(section.failures[0].contains("MAC length capped at 32 bytes"));
        let mut out = Out::new();
        format_neighbors(&[], &HashMap::new(), &mut out);
        assert_eq!(
            out.finish().body,
            "Status: No relevant dynamic ARP entries found.\r\n"
        );
        // Expected strings were checked against this PC's System.Net.IPAddress.ToString.
        for (input, expected) in [
            ("::192.0.2.1", "::192.0.2.1"),
            ("::2", "::2"),
            ("::0.0.1.1", "::101"),
            ("::ffff:192.0.2.1", "::ffff:192.0.2.1"),
            ("fe80::5efe:192.0.2.1", "fe80::5efe:192.0.2.1"),
            ("fe80::200:5efe:192.0.2.1", "fe80::200:5efe:c000:201"),
            ("2001:db8::192.0.2.1", "2001:db8::c000:201"),
        ] {
            assert_eq!(ip_text(input.parse().expect("fixture IP")), expected);
        }
    }

    #[test]
    fn arp_exe_fixture_preserves_case_spacing_filters_and_empty_status() {
        #[derive(serde::Deserialize)]
        struct Case {
            output: String,
            expected: String,
        }
        let cases: Vec<Case> = serde_json::from_str(include_str!(
            "../../tests/fixtures/wp-09/arp-exe-format.json"
        ))
        .expect("fabricated arp.exe fixture");
        for case in cases {
            let mut out = Out::new();
            format_arp_exe(&case.output, &mut out);
            assert_eq!(out.finish().body, case.expected);
        }
    }
}
