//! Neighbor-cache formatting and the bounded legacy arp.exe fallback.

use crate::{
    hw::{self, Ctx},
    report::Out,
    win::{
        self,
        iphlp::{self, Neighbor},
        process,
    },
};
use std::{
    collections::{BTreeMap, HashMap},
    net::{IpAddr, Ipv4Addr, Ipv6Addr},
    time::Duration,
};
use windows::Win32::Networking::WinSock::NlnsIncomplete;

const EMPTY_STATUS: &str = "No relevant dynamic ARP entries found.";

enum Cache {
    Native(Vec<Neighbor>),
    ArpExe(process::Output),
}

/// Collects this hardware section through the shared output builder.
pub fn collect(_ctx: &Ctx, out: &mut Out) -> Result<(), win::Error> {
    let native = || iphlp::neighbor_table().map(Cache::Native);
    let fallback = || arp_exe().map(Cache::ArpExe);
    // C# parity: ArpInfo.cs:24-40. A successful empty table must not trigger arp.exe.
    match hw::first_ok(out, "ARP", &[("native", &native), ("arp.exe", &fallback)]) {
        Ok(Cache::Native(entries)) => {
            if !entries.iter().any(relevant_neighbor) {
                format_neighbors(&entries, &HashMap::new(), out);
                out.trim_end();
                return Ok(());
            }
            let names = match iphlp::interface_names() {
                Ok(names) => names,
                Err(error) => {
                    out.fallback_failed("GetAdaptersAddresses", &error);
                    format_neighbors(&entries, &HashMap::new(), out);
                    out.text(&format!("Interface name lookup: {error}"));
                    out.trim_end();
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
    // C# parity: ArpInfo.cs:29,36,40. Trim the body, preserving identifier/source records.
    out.trim_end();
    Ok(())
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
        let address = &entry.physical_address;
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
    use windows::Win32::Networking::WinSock::NL_NEIGHBOR_STATE;

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
        out.trim_end();
        let section = out.finish();
        assert_eq!(section.body, fixture.expected.trim_end());
        assert_eq!(section.ids.len(), 14);
        assert!(section.failures.is_empty());
        // The native wrapper owns the MAC cap; keep only provider-format checks here.
        let mut out = Out::new();
        format_neighbors(&[], &HashMap::new(), &mut out);
        out.trim_end();
        assert_eq!(
            out.finish().body,
            "Status: No relevant dynamic ARP entries found."
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
            out.trim_end();
            assert_eq!(out.finish().body, case.expected.trim_end());
        }
    }

    #[test]
    #[ignore = "reads real identifiers; redirect stdout into the private golden/wp-09 directory"]
    fn wp09_compare_native_and_arp_exe() {
        if win::security::is_admin() {
            return;
        }
        let started = std::time::Instant::now();
        let entries = iphlp::neighbor_table().expect("live native neighbor table");
        let names = iphlp::interface_names().expect("live dual-stack interface names");
        let mut native = Out::new();
        format_neighbors(&entries, &names, &mut native);
        native.trim_end();
        let native_ms = started.elapsed().as_millis();
        let started = std::time::Instant::now();
        let fallback = arp_exe().expect("live arp.exe");
        let mut legacy = Out::new();
        format_arp_exe(&fallback.stdout, &mut legacy);
        legacy.trim_end();
        let fallback_ms = started.elapsed().as_millis();
        println!(
            "WP09_NATIVE_BEGIN\n{}\nWP09_NATIVE_END",
            native.finish().body
        );
        println!(
            "WP09_ARP_EXE_BEGIN\n{}\nWP09_ARP_EXE_END",
            legacy.finish().body
        );
        println!(
            "WP09_RAW_ARP_EXE_BEGIN\n{}\nWP09_RAW_ARP_EXE_END",
            fallback.stdout
        );
        println!(
            "Native: {native_ms} ms; arp.exe: {fallback_ms} ms; stderr={:?}",
            fallback.stderr
        );
        println!("WP09_NATIVE_ROWS_BEGIN");
        for entry in entries.iter().filter(|entry| relevant_neighbor(entry)) {
            println!("{entry:?}");
        }
        println!("WP09_NATIVE_ROWS_END");
        println!("WP09_INTERFACE_ROWS_BEGIN");
        for row in iphlp::interface_table().expect("live native interface table") {
            println!("{row:?}");
        }
        println!("WP09_INTERFACE_ROWS_END");
        let ctx = Ctx::new();
        let mut out = Out::new();
        collect(&ctx, &mut out).expect("live ARP provider");
        let section = out.finish();
        assert_eq!(section.source, "native");
        assert!(section.failures.is_empty(), "{:?}", section.failures);
        assert_eq!(section.body, section.body.trim_end());
        for error in win::take_recorded() {
            println!("Helper diagnostic: {error}");
        }
    }
}
