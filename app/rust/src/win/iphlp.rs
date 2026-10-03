//! Owned IpHelper table snapshots and dual-stack interface names.

use super::{Error, Result, record, wide};
use std::{
    collections::HashMap,
    ffi::c_void,
    mem::{align_of, size_of},
    net::{IpAddr, Ipv4Addr, Ipv6Addr},
    ptr::{self, NonNull},
    slice,
};
use windows::{
    Win32::{
        Foundation::{ERROR_BUFFER_OVERFLOW, ERROR_NO_DATA},
        NetworkManagement::IpHelper::{
            FreeMibTable, GAA_FLAG_SKIP_ANYCAST, GAA_FLAG_SKIP_DNS_SERVER, GAA_FLAG_SKIP_MULTICAST,
            GAA_FLAG_SKIP_UNICAST, GetAdaptersAddresses, GetIfTable2, GetIpNetTable2,
            IP_ADAPTER_ADDRESSES_LH, MIB_IF_ROW2, MIB_IF_TABLE2, MIB_IPNET_ROW2, MIB_IPNET_TABLE2,
        },
        Networking::WinSock::{AF_INET, AF_INET6, AF_UNSPEC, NL_NEIGHBOR_STATE},
    },
    core::GUID,
};

/// An owned interface row, including its stable identity and both MAC addresses.
#[derive(Clone, Debug, Default)]
pub struct Interface {
    pub guid: GUID,
    pub luid: u64,
    pub index: u32,
    pub alias: String,
    pub physical_address_length: u32,
    pub physical_address: [u8; 32],
    pub permanent_physical_address: [u8; 32],
}

/// An owned IPv4 or IPv6 neighbor with its SDK state and bounded physical address.
#[derive(Debug)]
pub struct Neighbor {
    pub interface_index: u32,
    pub ip: IpAddr,
    pub physical_address: Vec<u8>,
    pub state: NL_NEIGHBOR_STATE,
}

struct MibTable(NonNull<c_void>);

impl Drop for MibTable {
    fn drop(&mut self) {
        // SAFETY: This guard uniquely owns the allocation returned by an IpHelper table API.
        unsafe { FreeMibTable(self.0.as_ptr()) };
    }
}

/// Copies GetIfTable2 rows and releases the native table before returning.
pub fn interface_table() -> Result<Vec<Interface>> {
    let mut raw: *mut MIB_IF_TABLE2 = ptr::null_mut();
    // SAFETY: raw is a writable output pointer; every returned allocation gets a guard.
    let status = unsafe { GetIfTable2(&mut raw) };
    let guard = NonNull::new(raw).map(|ptr| MibTable(ptr.cast()));
    status
        .ok()
        .map_err(|error| Error::from_win("GetIfTable2", error))?;
    let _guard = guard.ok_or_else(|| Error::msg("GetIfTable2", "null table on success"))?;
    // SAFETY: A successful GetIfTable2 returns an initialized table header, held by _guard.
    let count = unsafe { (*raw).NumEntries as usize };
    if count > isize::MAX as usize / size_of::<MIB_IF_ROW2>() {
        return Err(Error::msg(
            "GetIfTable2",
            "table length exceeds addressable memory",
        ));
    }
    // SAFETY: The SDK Table field has the correct alignment; the OS allocated count rows.
    // addr_of! avoids creating a reference to the SDK's one-element flexible-array marker.
    let rows =
        unsafe { slice::from_raw_parts(ptr::addr_of!((*raw).Table).cast::<MIB_IF_ROW2>(), count) };
    Ok(rows
        .iter()
        .map(|row| Interface {
            guid: row.InterfaceGuid,
            // SAFETY: NET_LUID's Value is the documented scalar view of this initialized union.
            luid: unsafe { row.InterfaceLuid.Value },
            index: row.InterfaceIndex,
            alias: wide::from_wide(&row.Alias),
            physical_address_length: row.PhysicalAddressLength,
            physical_address: row.PhysicalAddress,
            permanent_physical_address: row.PermanentPhysicalAddress,
        })
        .collect())
}

/// Copies both IP families from GetIpNetTable2, preserving states for provider filtering.
pub fn neighbor_table() -> Result<Vec<Neighbor>> {
    let mut raw: *mut MIB_IPNET_TABLE2 = ptr::null_mut();
    // SAFETY: AF_UNSPEC requests both families; raw is writable and receives an owned table.
    let status = unsafe { GetIpNetTable2(AF_UNSPEC, &mut raw) };
    let guard = NonNull::new(raw).map(|ptr| MibTable(ptr.cast()));
    status
        .ok()
        .map_err(|error| Error::from_win("GetIpNetTable2", error))?;
    let _guard = guard.ok_or_else(|| Error::msg("GetIpNetTable2", "null table on success"))?;
    // SAFETY: The successful table header is initialized and remains live under _guard.
    let count = unsafe { (*raw).NumEntries as usize };
    if count > isize::MAX as usize / size_of::<MIB_IPNET_ROW2>() {
        return Err(Error::msg(
            "GetIpNetTable2",
            "table length exceeds addressable memory",
        ));
    }
    // SAFETY: The SDK field supplies alignment/padding; the OS allocated count complete rows.
    let rows = unsafe {
        slice::from_raw_parts(ptr::addr_of!((*raw).Table).cast::<MIB_IPNET_ROW2>(), count)
    };
    Ok(rows.iter().filter_map(neighbor).collect())
}

fn neighbor(row: &MIB_IPNET_ROW2) -> Option<Neighbor> {
    // SAFETY: si_family is the common discriminator of every SOCKADDR_INET variant.
    let family = unsafe { row.Address.si_family };
    let ip = match family {
        AF_INET => {
            // SAFETY: AF_INET selects Ipv4; S_addr's memory bytes are in network order.
            let bytes = unsafe { row.Address.Ipv4.sin_addr.S_un.S_addr.to_ne_bytes() };
            IpAddr::V4(Ipv4Addr::from(bytes))
        }
        AF_INET6 => {
            // SAFETY: AF_INET6 selects Ipv6 and IN6_ADDR's initialized 16-byte view.
            let bytes = unsafe { row.Address.Ipv6.sin6_addr.u.Byte };
            // C# parity: IpHlpApi.cs:107-112. Do not include sin6_scope_id in report IPs.
            IpAddr::V6(Ipv6Addr::from(bytes))
        }
        _ => return None,
    };
    let length = row.PhysicalAddressLength as usize;
    if length > row.PhysicalAddress.len() {
        record(Error::msg(
            "GetIpNetTable2",
            "MAC length capped at 32 bytes",
        ));
    }
    Some(Neighbor {
        interface_index: row.InterfaceIndex,
        ip,
        physical_address: row.PhysicalAddress[..length.min(row.PhysicalAddress.len())].to_vec(),
        state: row.State,
    })
}

/// Resolves FriendlyName by both nonzero IfIndex and Ipv6IfIndex, with bounded retries.
pub fn interface_names() -> Result<HashMap<u32, String>> {
    const MAX_BYTES: usize = 16 * 1024 * 1024;
    let mut bytes = 15 * 1024_usize;
    for _ in 0..3 {
        if bytes > MAX_BYTES {
            return Err(Error::msg(
                "GetAdaptersAddresses",
                "adapter buffer exceeds 16 MiB",
            ));
        }
        // u64 storage supplies SDK alignment and initializes every byte, including padding.
        const _: () = assert!(align_of::<IP_ADAPTER_ADDRESSES_LH>() <= align_of::<u64>());
        let mut buffer = vec![0_u64; bytes.div_ceil(size_of::<u64>())];
        let mut size = std::mem::size_of_val(buffer.as_slice()) as u32;
        // SAFETY: buffer is aligned, writable for size bytes and lives through name copying.
        let status = unsafe {
            GetAdaptersAddresses(
                u32::from(AF_UNSPEC.0),
                GAA_FLAG_SKIP_UNICAST
                    | GAA_FLAG_SKIP_ANYCAST
                    | GAA_FLAG_SKIP_MULTICAST
                    | GAA_FLAG_SKIP_DNS_SERVER,
                None,
                Some(buffer.as_mut_ptr().cast()),
                &mut size,
            )
        };
        if status == ERROR_BUFFER_OVERFLOW.0 {
            bytes = (size as usize).max(bytes.saturating_mul(2));
            continue;
        }
        if status == ERROR_NO_DATA.0 {
            return Ok(HashMap::new());
        }
        windows::Win32::Foundation::WIN32_ERROR(status)
            .ok()
            .map_err(|error| Error::from_win("GetAdaptersAddresses", error))?;
        return adapter_names(&buffer);
    }
    Err(Error::msg(
        "GetAdaptersAddresses",
        "adapter buffer changed during three attempts",
    ))
}

fn adapter_names(buffer: &[u64]) -> Result<HashMap<u32, String>> {
    let start = buffer.as_ptr() as usize;
    let end = start + std::mem::size_of_val(buffer);
    let mut row = buffer.as_ptr().cast::<IP_ADAPTER_ADDRESSES_LH>();
    let mut names = HashMap::new();
    // At most one full adapter row per storage slot; this also bounds cyclic Next lists.
    for _ in 0..std::mem::size_of_val(buffer) / size_of::<IP_ADAPTER_ADDRESSES_LH>() {
        if row.is_null() {
            return Ok(names);
        }
        let address = row as usize;
        if address < start
            || address > end.saturating_sub(size_of::<IP_ADAPTER_ADDRESSES_LH>())
            || !address.is_multiple_of(align_of::<IP_ADAPTER_ADDRESSES_LH>())
        {
            return Err(Error::msg(
                "GetAdaptersAddresses",
                "adapter row outside buffer",
            ));
        }
        // SAFETY: The OS initialized this linked row, and its full range/alignment was checked.
        let adapter = unsafe { &*row };
        // SAFETY: The Anonymous view contains the documented Length and IfIndex fields.
        let index = unsafe { adapter.Anonymous1.Anonymous.IfIndex };
        if !adapter.FriendlyName.is_null() {
            let address = adapter.FriendlyName.0 as usize;
            if address < start || address >= end || !address.is_multiple_of(align_of::<u16>()) {
                return Err(Error::msg(
                    "GetAdaptersAddresses",
                    "friendly name outside buffer",
                ));
            }
            // SAFETY: The aligned UTF-16 range stays inside the live OS-filled allocation.
            let units =
                unsafe { slice::from_raw_parts(adapter.FriendlyName.0, (end - address) / 2) };
            let length = units
                .iter()
                .position(|unit| *unit == 0)
                .ok_or_else(|| Error::msg("GetAdaptersAddresses", "unterminated friendly name"))?;
            let name = wide::from_wide(&units[..length]);
            for index in [index, adapter.Ipv6IfIndex] {
                if index != 0 {
                    names.insert(index, name.clone());
                }
            }
        }
        row = adapter.Next;
    }
    if row.is_null() {
        Ok(names)
    } else {
        Err(Error::msg(
            "GetAdaptersAddresses",
            "cyclic or oversized adapter list",
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use windows::Win32::{
        NetworkManagement::IpHelper::{IP_ADAPTER_ADDRESSES_LH_0, IP_ADAPTER_ADDRESSES_LH_0_0},
        Networking::WinSock::{
            IN_ADDR, IN_ADDR_0, IN6_ADDR, IN6_ADDR_0, NlnsPermanent, SOCKADDR_IN, SOCKADDR_IN6,
            SOCKADDR_IN6_0, SOCKADDR_INET,
        },
    };
    use windows::core::PWSTR;

    #[test]
    fn native_sockaddr_projection_preserves_bytes_state_and_mac_bounds() {
        let mut row = MIB_IPNET_ROW2 {
            InterfaceIndex: 7,
            PhysicalAddressLength: 6,
            State: NlnsPermanent,
            Address: SOCKADDR_INET {
                Ipv4: SOCKADDR_IN {
                    sin_family: AF_INET,
                    sin_addr: IN_ADDR {
                        S_un: IN_ADDR_0 {
                            S_addr: u32::from_ne_bytes([192, 0, 2, 11]),
                        },
                    },
                    ..Default::default()
                },
            },
            ..Default::default()
        };
        row.PhysicalAddress[..6].copy_from_slice(&[0x3c, 0xfd, 0xfe, 0x64, 0x19, 0x82]);
        let entry = neighbor(&row).expect("fabricated IPv4 row");
        assert_eq!(entry.ip, "192.0.2.11".parse::<IpAddr>().unwrap());
        assert_eq!(entry.interface_index, 7);
        assert_eq!(entry.state, NlnsPermanent);
        assert_eq!(entry.physical_address, row.PhysicalAddress[..6]);
        let ipv6: Ipv6Addr = "fe80::7a19:4c2f:8e34:16b5".parse().unwrap();
        row.Address = SOCKADDR_INET {
            Ipv6: SOCKADDR_IN6 {
                sin6_family: AF_INET6,
                sin6_addr: IN6_ADDR {
                    u: IN6_ADDR_0 {
                        Byte: ipv6.octets(),
                    },
                },
                Anonymous: SOCKADDR_IN6_0 { sin6_scope_id: 7 },
                ..Default::default()
            },
        };
        row.PhysicalAddressLength = 33;
        let entry = neighbor(&row).expect("fabricated IPv6 row");
        assert_eq!(entry.ip, IpAddr::V6(ipv6));
        assert!(!entry.ip.to_string().contains('%'));
        assert_eq!(entry.physical_address.len(), 32);
        row.Address = SOCKADDR_INET {
            si_family: AF_UNSPEC,
        };
        assert!(neighbor(&row).is_none());
    }

    #[test]
    fn adapter_name_projection_maps_ipv4_ipv6_and_distinct_dual_stack_indices() {
        let mut buffer = vec![0_u64; 1024];
        let base = buffer.as_mut_ptr().cast::<IP_ADAPTER_ADDRESSES_LH>();
        let cases = [
            (7, 0, "Ethernet"),
            (0, 19, "vEthernet (IPv6)"),
            (23, 31, "Wi-Fi Écran"),
        ];
        for (ordinal, (ipv4, ipv6, name)) in cases.iter().enumerate() {
            let units = wide::to_wide(name);
            // SAFETY: Each fabricated row and its UTF-16 string occupy disjoint aligned
            // ranges inside the zeroed buffer; the last Next is null and pointers stay live.
            unsafe {
                let name_ptr = buffer.as_mut_ptr().cast::<u16>().add(2048 + ordinal * 128);
                ptr::copy_nonoverlapping(units.as_ptr(), name_ptr, units.len());
                base.add(ordinal).write(IP_ADAPTER_ADDRESSES_LH {
                    Anonymous1: IP_ADAPTER_ADDRESSES_LH_0 {
                        Anonymous: IP_ADAPTER_ADDRESSES_LH_0_0 {
                            Length: size_of::<IP_ADAPTER_ADDRESSES_LH>() as u32,
                            IfIndex: *ipv4,
                        },
                    },
                    Ipv6IfIndex: *ipv6,
                    FriendlyName: PWSTR(name_ptr),
                    Next: if ordinal + 1 < cases.len() {
                        base.add(ordinal + 1)
                    } else {
                        ptr::null_mut()
                    },
                    ..Default::default()
                });
            }
        }
        assert_eq!(
            adapter_names(&buffer).expect("fabricated adapter list"),
            HashMap::from([
                (7, "Ethernet".to_owned()),
                (19, "vEthernet (IPv6)".to_owned()),
                (23, "Wi-Fi Écran".to_owned()),
                (31, "Wi-Fi Écran".to_owned()),
            ])
        );
    }
}
