# Router and ARP-Table Isolation Guide

> [!NOTE]
> **TL;DR:** Places a router you control between your PC and the primary network, so your PC's ARP table shows the isolation router's MAC as gateway instead of your home router's. Also changes the SSID/BSSID your Wi-Fi reports and the DHCP lease you get.
> Who reads it: the local network, DHCP servers, and any fingerprinting stack that records gateway MAC, SSID, or BSSID.
> **Status:** procedures follow cited standards and vendor documentation **[A]**. Not tested on every router model or firmware build.
> **Risk:** low if you stay in Router/WISP mode and keep a configuration backup plus physical reset access. Use only equipment and networks that you own or administer.

> [!NOTE]
> In this project, "ARP spoofing" means placing a router that you control between your PC and the primary network so your own PC sees a different first-hop gateway MAC address. It does **not** mean ARP poisoning. This guide contains no instructions for forging ARP replies, intercepting traffic, or attacking another network.

Evidence grades appear inline. See [How to read these guides](../getting-started/getting-started.md#how-to-read-these-guides). This guide uses [A] evidence for the procedures below.

## Table of Contents

- [Overview](#overview)
- [What this changes](#what-this-changes)
- [Requirements](#requirements)
- [Options](#options)
- [Steps](#steps)
- [Verify with HWIDChecker](#verify-with-hwidchecker)
- [Troubleshooting](#troubleshooting)
- [Sources](#sources)

## Overview

IPv4 ARP resolves an on-link IP address to a link-layer address. For traffic outside the local subnet, the PC resolves the MAC address of its first-hop gateway. It does not learn the MAC addresses of routers farther along the Internet path. [A] [RFC 826](https://www.rfc-editor.org/rfc/rfc826.html)

Windows exposes several pieces of local network context to programs:

- The IPv4 ARP cache contains IP-to-MAC mappings for on-link neighbors. `arp -a` displays it. [A] [Microsoft `arp`](https://learn.microsoft.com/en-us/windows-server/administration/windows-commands/arp)
- The Windows neighbor cache contains IPv4 and IPv6 IP-to-link-layer mappings. `Get-NetNeighbor` displays both families. [A] [Microsoft `Get-NetNeighbor`](https://learn.microsoft.com/en-us/powershell/module/nettcpip/get-netneighbor?view=windowsserver2025-ps)
- The route table exposes the default route and next-hop gateway. [A] [Microsoft `Get-NetRoute`](https://learn.microsoft.com/en-us/powershell/module/nettcpip/get-netroute?view=windowsserver2025-ps)
- `ipconfig /all` exposes adapter addresses, the default gateway, DHCP configuration, and DNS configuration. [A] [Microsoft `ipconfig`](https://learn.microsoft.com/en-us/windows-server/administration/windows-commands/ipconfig)
- A Wi-Fi client can read the SSID and BSSID of the access point that it joined. The BSSID is the access point's link-layer identifier. [A] [Microsoft `WLAN_ASSOCIATION_ATTRIBUTES`](https://learn.microsoft.com/en-us/windows/win32/api/wlanapi/ns-wlanapi-wlan_association_attributes)

These values describe the PC's current network environment. They are not permanent global hardware identifiers, and this guide does not claim that any particular program collects or uploads them. A stable gateway MAC or BSSID can still be one input to a broader local-environment fingerprint.

The isolation design creates a routed boundary:

```text
Internet
   |
Primary router or hotspot
   |
   |  WAN, Ethernet uplink, or Wi-Fi repeater uplink
   v
Isolation router in Router/WISP mode
   |
   |  LAN Ethernet or the isolation router's own Wi-Fi
   v
Target PC
```

The PC now resolves the isolation router's **LAN-side** MAC as its default gateway. The primary router sees the isolation router's **WAN or repeater-uplink** MAC. These are different interfaces and often have different MAC addresses. GL.iNet documents separate MACs for WAN, LAN, and Wi-Fi interfaces. [A] [GL.iNet interface MAC address reference](https://docs.gl-inet.com/router/en/4/faq/how_can_i_know_the_lan_wifi_mac/)

> [!CAUTION]
> Keep the isolation device in a routed mode. Access Point, WDS, extender, or bridge modes can leave the PC on the primary router's Layer 2 network. GL.iNet documents that Router mode provides NAT, DHCP, and firewall functions, while Access Point and Bridge modes disable routing functions. [A] [GL.iNet Network Mode](https://docs.gl-inet.com/router/en/4/interface_guide/network_mode/)

## What this changes

| Signal visible to the PC | Expected result behind a routed isolation device | Evidence |
|---|---|---|
| IPv4 default-gateway IP | Becomes the isolation router's LAN IP | [A] |
| IPv4 default-gateway MAC | Becomes the isolation router's LAN or bridge MAC | [A] |
| Other IPv4 ARP neighbors | Limited to devices on the isolated LAN, plus normal broadcast or multicast entries | [A] |
| DHCP server and lease range | Usually become the isolation router's DHCP service and private subnet | [A] |
| Router hostname exposed by local services | Can be replaced with a neutral hostname on the isolation device | [A] |
| IPv6 neighbors and router | Must be checked separately; IPv6 uses Neighbor Discovery, not ARP | [A] |
| Wi-Fi SSID and BSSID | Become those of the isolation access point only if the PC joins its Wi-Fi | [A] |
| PC NIC MAC | Unchanged. See the [MAC spoofing guide](../mac-spoofing/mac-spoofing.md) for that separate layer. | [A] |
| Public IP address | Usually unchanged unless the isolation device also uses a VPN or different uplink | [A] |
| Account, browser, application, disk, TPM, and other identifiers | Unchanged | [A] |

IPv6 is a separate path. IPv6 Neighbor Discovery finds routers and link-layer addresses and maintains a Neighbor Cache. An IPv4-only ARP check does not prove that IPv6 is isolated. [A] [RFC 4861](https://www.rfc-editor.org/rfc/rfc4861.html)

> [!TIP]
> Three MAC roles are easy to confuse:
>
> - **LAN or `br-lan` MAC:** the gateway MAC that a wired PC normally sees in ARP.
> - **WAN or repeater MAC:** the MAC that the upstream router or hotspot sees.
> - **Wi-Fi BSSID:** the access-point identifier that a wireless PC sees. It may differ from the gateway MAC.

## Requirements

- Windows 10 or Windows 11 on the target PC.
- One intermediate device:
  - a GL.iNet travel router,
  - another router running OpenWrt, or
  - a Raspberry Pi 4 Model B running Raspberry Pi OS Bookworm or newer.
- Two network sides: one uplink to the primary network and one downstream link to the target PC.
- Administrative access to the intermediate device.
- A configuration backup and physical reset access.
- A private LAN subnet that does not overlap the primary network. This guide uses `192.168.73.0/24`, with `192.168.73.1` as the isolation gateway.
- A neutral hostname that does not contain your name, location, or device purpose.

Use a strong router admin password. Do not expose the router's admin interface or SSH service to the WAN.

## Options

| Option | Best for | Uplink | Downstream | Notes |
|---|---|---|---|---|
| GL.iNet travel router | Easiest setup | Ethernet WAN or Wi-Fi repeater | Ethernet LAN or its own Wi-Fi | Use Router/WISP mode. GL.iNet's normal MAC controls are uplink-facing. |
| Any supported OpenWrt router | Maximum control | Ethernet or supported Wi-Fi uplink | LAN or routed Wi-Fi | Set the actual LAN device or `br-lan` MAC with OpenWrt's `macaddr` option. |
| Raspberry Pi 4 Model B | Reusable Linux router | Built-in Wi-Fi | Ethernet `eth0` | NetworkManager shared mode supplies IPv4 DHCP, DNS forwarding, forwarding, and NAT. |

### Option A: GL.iNet travel router

GL.iNet firmware 4 documents Router mode, Ethernet WAN, repeater/WISP uplinks, LAN DHCP, and Factory, Clone, or Random MAC modes for WAN and repeater interfaces. [A] [Network Mode](https://docs.gl-inet.com/router/en/4/interface_guide/network_mode/), [Ethernet uplink](https://docs.gl-inet.com/router/en/4/interface_guide/internet_ethernet/), [Repeater uplink](https://docs.gl-inet.com/router/en/4/interface_guide/internet_repeater/), [LAN](https://docs.gl-inet.com/router/en/4/interface_guide/lan/)

This is the simplest option for models such as the GL-SFT1200. Menu names differ by firmware version. Use the version selector in GL.iNet's documentation if your interface does not match the current guide.

> [!NOTE]
> The GL.iNet **MAC Mode** control for Ethernet WAN or Repeater changes the uplink MAC. That is useful when you also want a different MAC presented to the upstream network. It does not, by itself, change the LAN gateway MAC stored in the PC's ARP cache. [A] [GL.iNet MAC Address](https://docs.gl-inet.com/router/en/4/interface_guide/mac_address/)

### Option B: any OpenWrt router

OpenWrt supports a `macaddr` value in a `config device` section. The value overrides the default MAC for the named device. A common LAN bridge name is `br-lan`, but device names vary. [A] [OpenWrt network configuration](https://openwrt.org/docs/guide-user/network/network_configuration), [OpenWrt bridged LAN example](https://openwrt.org/docs/guide-user/network/dsa/dsa-common-config-1-bridging-all-lan-ports)

Do not use a copied command that targets `network.@device[0]`. Anonymous section order is device-specific and can select the wrong interface.

### Option C: Raspberry Pi 4 Model B

Raspberry Pi OS Bookworm uses NetworkManager by default. NetworkManager's IPv4 `shared` method starts DHCP and DNS forwarding on the downstream interface and configures NAT to the current default connection. [A] [Raspberry Pi networking](https://www.raspberrypi.com/documentation/computers/configuration.html), [NetworkManager settings](https://www.networkmanager.dev/docs/api/latest/nm-settings-nmcli.html)

The topology used here is:

```text
Primary Wi-Fi -> Raspberry Pi wlan0 -> routed/NAT boundary -> Raspberry Pi eth0 -> target PC
```

Do not create a network bridge. A bridge intentionally keeps clients on the parent Layer 2 network and defeats this guide's gateway-MAC boundary.

The commands below assume that the Pi's built-in Wi-Fi is `wlan0` and its built-in Ethernet port is `eth0`. Confirm both names with `nmcli device status` and substitute the names shown on your system. A Raspberry Pi 4 Model B has one built-in Ethernet port. An all-wired topology therefore needs a second supported network interface, such as a USB Ethernet adapter. [A] [Raspberry Pi computer hardware](https://www.raspberrypi.com/documentation/computers/raspberry-pi.html)

## Steps

### 1. Record the baseline on Windows [A]

Open PowerShell. Save the output somewhere outside this repository if it contains real network identifiers.

```powershell
ipconfig /all

Get-NetRoute |
    Where-Object DestinationPrefix -In '0.0.0.0/0', '::/0' |
    Format-Table AddressFamily, InterfaceAlias, DestinationPrefix, NextHop, RouteMetric

arp -a

Get-NetNeighbor -AddressFamily IPv4 |
    Format-Table InterfaceAlias, IPAddress, LinkLayerAddress, State

Get-NetNeighbor -AddressFamily IPv6 |
    Format-Table InterfaceAlias, IPAddress, LinkLayerAddress, State
```

If the PC is using Wi-Fi, also record its current association:

```powershell
netsh wlan show interfaces
```

Microsoft documents `netsh wlan show interfaces` as a Windows Wi-Fi status command. [A] [Microsoft `netsh wlan`](https://learn.microsoft.com/en-us/windows-server/administration/windows-commands/netsh-wlan)

### 2. Build the routed topology [A]

1. Disconnect the target PC from the primary router's Ethernet and Wi-Fi.
2. Connect the isolation device's uplink to the primary network.
3. Connect the target PC only to the isolation device's LAN port or its own Wi-Fi.
4. Do not connect other client devices to the isolated LAN while validating it.
5. Confirm that the isolation device is in Router or WISP mode, not Access Point, WDS, extender, or bridge mode.

Keeping only the target PC on the downstream LAN reduces the number of ordinary on-link neighbors that can appear in its neighbor cache. It does not make the cache empty. The isolation router itself must remain visible as the gateway.

### 3A. Configure a GL.iNet travel router [A]

1. Sign in to the router's local web Admin Panel.
2. Open **NETWORK -> Network Mode** and select **Router**.
3. Configure one uplink:
   - For Ethernet, connect the primary router to the GL.iNet **WAN** port and use **INTERNET -> Ethernet**.
   - For Wi-Fi, use **INTERNET -> Repeater**. GL.iNet documents that repeater mode uses WISP behavior by default, creating its own subnet and firewall boundary.
4. Open **NETWORK -> LAN**.
5. Set the router IP to `192.168.73.1` with netmask `255.255.255.0`.
6. Keep the LAN DHCP server enabled. Reconnect the PC after applying the subnet change.
7. In LuCI, set a neutral system hostname. OpenWrt stores this as `option hostname` in `/etc/config/system`. [A] [OpenWrt system configuration](https://openwrt.org/docs/guide-user/base-system/system_configuration)
8. If you also want to change what the upstream network sees:
   - On Ethernet WAN, open **NETWORK -> Ethernet Port** or **Port Management** and set the WAN **MAC Mode** to **Random** or **Clone**.
   - On a Wi-Fi repeater uplink, edit the saved repeater network and set **MAC Mode** to **Random** or **Clone**.
9. Verify the downstream LAN MAC. On current GL.iNet firmware, different interfaces have separate MAC addresses. Do not assume the WAN MAC is the LAN MAC.

For a wired target PC, the value that matters to ARP isolation is normally the `br-lan` or LAN-device MAC. To override it:

1. Back up the router configuration. GL.iNet documents **SYSTEM -> Advanced Settings -> LuCI -> System -> Backup / Flash Firmware -> Generate archive**. [A] [GL.iNet upgrade and backup procedure](https://docs.gl-inet.com/router/en/4/tutorials/how_to_upgrade_downgrade_router/)
2. Open **SYSTEM -> Advanced Settings** and select **Go To LuCI**. [A] [GL.iNet Advanced Settings](https://docs.gl-inet.com/router/en/4/interface_guide/advanced_settings/)
3. In LuCI, open **Network -> Interfaces -> Devices** and identify the device used by the LAN interface. It is commonly `br-lan`, but confirm it on your router.
4. Configure a MAC override on that device. Preserve the router vendor's first three octets and change only the device-specific last three octets. For example, `94:83:C4:7A:2E:19` uses a GL.iNet OUI shown in official documentation with a fabricated device-specific suffix. Do not reuse this exact example on more than one interface.
5. Save and apply. Reconnect the PC, then verify the new gateway mapping before continuing.

If LuCI does not expose a MAC field, use GL.iNet's documented local SSH access and add only the `option macaddr` line to the existing LAN `config device` section in `/etc/config/network`. Do not replace the device's existing bridge or port lines. [A] [GL.iNet SSH login](https://docs.gl-inet.com/router/en/4/tutorials/ssh_log_in_to_the_router/), [OpenWrt `macaddr` option](https://openwrt.org/docs/guide-user/network/network_configuration)

Example excerpt:

```text
config device
        option name 'br-lan'
        option type 'bridge'
        # Keep the router's existing list ports lines here.
        option macaddr '94:83:C4:7A:2E:19'
```

> [!CAUTION]
> Edit the existing LAN device only after confirming its name. Do not paste the whole example over your configuration. A wrong device name or removed bridge-port line can disconnect the router.

If the PC connects to the GL.iNet router over Wi-Fi, also consider GL.iNet's **Randomized BSSID** setting. GL.iNet documents that this feature has been available since firmware 4.6 and renews the generated BSSID at each boot. This changes the Wi-Fi access-point identifier, which is separate from the ARP gateway MAC. Availability still varies by model and firmware. [A] [GL.iNet Randomized BSSID](https://docs.gl-inet.com/router/en/4/interface_guide/wireless_v4.8/#randomized-bssid)

### 3B. Configure another OpenWrt router [A]

1. Keep the router in its default routed gateway role with LAN DHCP, firewall, and NAT enabled.
2. Connect the primary network to the WAN interface.
3. Connect the target PC to a LAN port or to a routed LAN Wi-Fi network.
4. Set a non-overlapping LAN subnet such as `192.168.73.1/24`.
5. In LuCI, open **Network -> Interfaces -> Devices** and identify the device used by the LAN interface.
6. Add a MAC override to that exact device. For a bridge, add `option macaddr` to the existing `config device` section whose `option name` is the confirmed bridge name.
7. Set a neutral hostname through LuCI or the existing `option hostname` in `/etc/config/system`. [A] [OpenWrt system configuration](https://openwrt.org/docs/guide-user/base-system/system_configuration)
8. Preserve the device vendor's OUI and change the last three octets. Ensure the new address is unique on both the upstream and downstream networks.
9. Save and apply, reconnect the PC, and verify the result.

OpenWrt's documented configuration shape is:

```text
config device
        option name '<confirmed-LAN-device>'
        option macaddr '<vendor-OUI:fabricated-device-suffix>'
```

The placeholders are intentional. Router port layouts differ, and guessing a physical interface can change the wrong MAC.

### 3C. Configure a Raspberry Pi 4 Model B [A]

This procedure uses Raspberry Pi OS Bookworm or newer and its default NetworkManager stack. It uses NetworkManager's documented shared mode instead of an older Netplan, nftables, and standalone dnsmasq sequence.

1. Install Raspberry Pi OS with the official Raspberry Pi Imager. Set a neutral hostname in Imager instead of using a name, location, or device purpose.
2. Connect a keyboard and display, or enable SSH during imaging.
3. Boot the Pi and update Raspberry Pi OS.
4. Connect `wlan0` to the primary Wi-Fi:

```bash
nmcli device wifi list
sudo nmcli --ask device wifi connect "<upstream-SSID>" ifname wlan0
```

5. Confirm that `wlan0` is the active default connection and `eth0` is available:

```bash
nmcli device status
nmcli connection show --active
```

6. Create a downstream Ethernet connection with IPv4 sharing:

```bash
sudo nmcli connection add \
  type ethernet \
  ifname eth0 \
  con-name isolated-lan \
  ipv4.method shared \
  ipv4.addresses 192.168.73.1/24 \
  ipv6.method disabled
```

NetworkManager shared mode enables forwarding, runs a DHCP server and DNS forwarder, and configures NAT from the downstream interface to the current default connection. [A] [NetworkManager `ipv4.method`](https://www.networkmanager.dev/docs/api/latest/nm-settings-nmcli.html)

7. Configure NetworkManager to randomize only the device-specific part of the Ethernet MAC while preserving the Pi's existing OUI:

```bash
sudo nmcli connection modify isolated-lan \
  802-3-ethernet.cloned-mac-address random \
  802-3-ethernet.generate-mac-address-mask "FE:FF:FF:00:00:00"
```

NetworkManager documents that this mask preserves the current OUI and randomizes the lower three bytes. It also keeps the address unicast. [A] [NetworkManager Ethernet settings](https://www.networkmanager.dev/docs/api/latest/nm-settings-nmcli.html)

> [!NOTE]
> `random` creates a new MAC on each connection. `stable` instead derives a repeatable value from the profile's `connection.stable-id` and a machine-dependent key. [A] [NetworkManager Ethernet settings](https://www.networkmanager.dev/docs/api/latest/nm-settings-nmcli.html)

8. Activate the downstream connection:

```bash
sudo nmcli connection up isolated-lan
nmcli -f GENERAL.DEVICE,GENERAL.HWADDR,GENERAL.STATE device show eth0
```

9. Connect the target PC directly to the Pi's Ethernet port.
10. Confirm that Windows receives an address in `192.168.73.0/24` and uses `192.168.73.1` as its default gateway.

This example disables IPv6 on the Pi's downstream profile because it documents an IPv4 isolation path only. If you need IPv6, configure a properly routed IPv6 downstream and verify it separately. Do not bridge the upstream Wi-Fi onto `eth0`.

### 4. Decide how to handle IPv6 [A]

For GL.iNet and OpenWrt, routed IPv6 can still preserve a Layer 3 boundary, but the exact behavior depends on prefix delegation and firmware configuration. Verify instead of assuming.

After connecting through the isolation device, run:

```powershell
Get-NetRoute -DestinationPrefix '::/0' |
    Format-Table InterfaceAlias, NextHop, RouteMetric

Get-NetNeighbor -AddressFamily IPv6 |
    Format-Table InterfaceAlias, IPAddress, LinkLayerAddress, State
```

The IPv6 router and neighbor MACs should belong to the isolation device's downstream link. If the primary router's link-local address or MAC remains visible on the target PC's active interface, the intended routed IPv6 boundary is not established. Check for bridge mode, IPv6 relay, or forwarded upstream Router Advertisements.

> [!WARNING]
> Do not treat a clean IPv4 ARP table as proof of IPv6 isolation. Either configure routed IPv6 correctly or disable IPv6 only on the isolated downstream segment after considering what applications will lose. There is no universal GL.iNet or OpenWrt IPv6 setting for every firmware and ISP.

## Verify with HWIDChecker

`HWIDChecker.exe` displays a section named **ARP INFO/CACHE**. The current source first queries the Windows IP Helper neighbor table. On that path it groups entries by interface and displays IPv4 and IPv6 neighbors whose MAC address passes its filters. It removes entries with Windows state value `1` (`NlnsIncomplete`), zero-length or all-zero MAC addresses, and broadcast or multicast MAC addresses. It does not explicitly remove Windows state value `0` (`NlnsUnreachable`). If the native query fails, it falls back to `arp -a`. The fallback is IPv4-only, locale-dependent, and displays only rows containing the English word `dynamic`. [A] [`arp.rs`](../../app/rust/src/hw/arp.rs), [`iphlp.rs`](../../app/rust/src/win/iphlp.rs), [Microsoft `NL_NEIGHBOR_STATE`](https://learn.microsoft.com/en-us/windows/win32/api/nldef/ne-nldef-nl_neighbor_state)

1. Run `HWIDChecker.exe` from the repository root.
2. Find **ARP INFO/CACHE**.
3. Locate the physical Ethernet or Wi-Fi interface used for the isolated connection. Ignore sections marked **(Virtual)** unless the target traffic intentionally uses that virtual adapter.
4. Compare the Windows default gateway from `ipconfig` or `Get-NetRoute` with the IP addresses in the HWIDChecker section.
5. Confirm that the gateway IP maps to the isolation device's LAN MAC, not the primary router's MAC.
6. If IPv6 is enabled, inspect the displayed IPv6 neighbors and also confirm them with `Get-NetNeighbor -AddressFamily IPv6`.
7. Generate ordinary traffic through the gateway and check again if the entry is missing. Neighbor caches are demand-driven and change over time.

> [!NOTE]
> HWIDChecker does not display the Windows neighbor state. An entry in its native output is therefore not proof that the neighbor is currently reachable. Use `Get-NetNeighbor` to check the state, and match the active default route to the correct interface and next hop.

Use the native Windows commands as the reference check:

```powershell
ipconfig /all
arp -a
Get-NetNeighbor -AddressFamily IPv4
Get-NetNeighbor -AddressFamily IPv6
```

> [!NOTE]
> `arp -a` can show normal static broadcast and IPv4 multicast mappings such as `FF-FF-FF-FF-FF-FF` and `01-00-5E-...`. HWIDChecker intentionally filters broadcast and multicast MACs. Their presence in raw `arp -a` output is not evidence that the primary router is visible.

If the PC uses the isolation router's Wi-Fi, verify the access point separately:

```powershell
netsh wlan show interfaces
```

The displayed SSID and BSSID should belong to the isolation access point. If the PC is connected by Ethernet, disconnect its Wi-Fi adapter during the test so an old or parallel wireless route does not confuse the result.

## Troubleshooting

### The default gateway is still the primary router

- Confirm that the PC is not still connected to the primary Wi-Fi.
- Confirm that the isolation device is in Router or WISP mode.
- Do not use Access Point, WDS, extender, drop-in gateway, or bridge mode for this topology.
- Confirm that the cable from the primary network enters the isolation device's WAN port and the PC uses a LAN port.

### The WAN MAC changed, but the PC still sees the old gateway MAC

This is expected if only the WAN or repeater MAC changed. The PC resolves the isolation device's LAN or `br-lan` MAC. Change and verify that downstream MAC separately.

### The gateway entry is missing

Use the network normally or contact the gateway, then inspect the cache again. ARP and Neighbor Discovery caches are temporary and demand-driven. Also make sure you are looking at the active physical interface, not a disconnected or virtual interface.

### Windows has no Internet access

- Check for overlapping subnets. The primary and isolation LANs must differ.
- Confirm that the isolation router received an upstream address and default route.
- Confirm that LAN DHCP is enabled.
- On Raspberry Pi, confirm that `wlan0` is connected, `eth0` is using `isolated-lan`, and the shared connection is active.

### Port forwarding or an inbound service stopped working

Two routed home routers normally create double NAT. Outbound browsing often works normally, but unsolicited inbound connections need mappings through each NAT layer. GL.iNet documents configuring port forwarding on every preceding router. [A] [GL.iNet port forwarding](https://docs.gl-inet.com/router/en/4/tutorials/how_to_set_up_port_forwarding/), [RFC 3022](https://www.rfc-editor.org/rfc/rfc3022.html)

If an inbound service is required, forward the port on both routers or put the **upstream modem/router** into an appropriate bridge or passthrough mode. Do not bridge the isolation router's downstream LAN, because that removes the boundary this guide creates.

### IPv6 still shows the primary router

The setup is not fully isolated for IPv6. Check for bridge mode, IPv6 relay, or upstream Router Advertisements reaching the PC. Configure routed IPv6 for the isolation device or disable IPv6 on the isolated downstream segment if that trade-off is acceptable.

### A GL.iNet menu does not match this guide

Firmware 4.5 and earlier used **NETWORK -> MAC Address**. Firmware 4.6 and later moved Ethernet and repeater MAC controls to their respective pages. Use the firmware-version selector in the GL.iNet documentation. [A] [GL.iNet MAC Address](https://docs.gl-inet.com/router/en/4/interface_guide/mac_address/)

### The LAN MAC change removed router access

Use a wired LAN connection and the configured LAN IP first. If that fails, restore the configuration backup or use the router's documented reset procedure. Do not keep applying interface changes without confirming the actual LAN device name.

## Sources

- [RFC 826: Ethernet Address Resolution Protocol](https://www.rfc-editor.org/rfc/rfc826.html)
- [RFC 4861: Neighbor Discovery for IPv6](https://www.rfc-editor.org/rfc/rfc4861.html)
- [RFC 3022: Traditional IP Network Address Translator](https://www.rfc-editor.org/rfc/rfc3022.html)
- [Microsoft `arp`](https://learn.microsoft.com/en-us/windows-server/administration/windows-commands/arp)
- [Microsoft `Get-NetNeighbor`](https://learn.microsoft.com/en-us/powershell/module/nettcpip/get-netneighbor?view=windowsserver2025-ps)
- [Microsoft `Get-NetRoute`](https://learn.microsoft.com/en-us/powershell/module/nettcpip/get-netroute?view=windowsserver2025-ps)
- [Microsoft `ipconfig`](https://learn.microsoft.com/en-us/windows-server/administration/windows-commands/ipconfig)
- [Microsoft `netsh wlan`](https://learn.microsoft.com/en-us/windows-server/administration/windows-commands/netsh-wlan)
- [Microsoft WLAN association attributes](https://learn.microsoft.com/en-us/windows/win32/api/wlanapi/ns-wlanapi-wlan_association_attributes)
- [Microsoft `NL_NEIGHBOR_STATE`](https://learn.microsoft.com/en-us/windows/win32/api/nldef/ne-nldef-nl_neighbor_state)
- [GL.iNet Network Mode](https://docs.gl-inet.com/router/en/4/interface_guide/network_mode/)
- [GL.iNet Ethernet uplink](https://docs.gl-inet.com/router/en/4/interface_guide/internet_ethernet/)
- [GL.iNet Repeater uplink](https://docs.gl-inet.com/router/en/4/interface_guide/internet_repeater/)
- [GL.iNet LAN](https://docs.gl-inet.com/router/en/4/interface_guide/lan/)
- [GL.iNet MAC Address](https://docs.gl-inet.com/router/en/4/interface_guide/mac_address/)
- [GL.iNet Advanced Settings](https://docs.gl-inet.com/router/en/4/interface_guide/advanced_settings/)
- [GL.iNet SSH login](https://docs.gl-inet.com/router/en/4/tutorials/ssh_log_in_to_the_router/)
- [GL.iNet interface MAC address reference](https://docs.gl-inet.com/router/en/4/faq/how_can_i_know_the_lan_wifi_mac/)
- [GL.iNet upgrade and backup procedure](https://docs.gl-inet.com/router/en/4/tutorials/how_to_upgrade_downgrade_router/)
- [GL.iNet Randomized BSSID](https://docs.gl-inet.com/router/en/4/interface_guide/wireless_v4.8/#randomized-bssid)
- [GL.iNet port forwarding](https://docs.gl-inet.com/router/en/4/tutorials/how_to_set_up_port_forwarding/)
- [OpenWrt network configuration](https://openwrt.org/docs/guide-user/network/network_configuration)
- [OpenWrt bridged LAN example](https://openwrt.org/docs/guide-user/network/dsa/dsa-common-config-1-bridging-all-lan-ports)
- [OpenWrt system configuration](https://openwrt.org/docs/guide-user/base-system/system_configuration)
- [Raspberry Pi networking and NetworkManager](https://www.raspberrypi.com/documentation/computers/configuration.html)
- [Raspberry Pi computer hardware](https://www.raspberrypi.com/documentation/computers/raspberry-pi.html)
- [Raspberry Pi Imager](https://www.raspberrypi.com/software/)
- [NetworkManager settings for `nmcli`](https://www.networkmanager.dev/docs/api/latest/nm-settings-nmcli.html)
- [NetworkManager `nmcli` manual](https://www.networkmanager.dev/docs/api/latest/nmcli.html)
- [HWIDChecker ARP provider](../../app/rust/src/hw/arp.rs)
- [HWIDChecker Windows neighbor-table wrapper](../../app/rust/src/win/iphlp.rs)
