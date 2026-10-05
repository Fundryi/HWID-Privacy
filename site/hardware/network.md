---
title: "NIC / MAC guide chooser"
description: "Choose a Windows override or your NIC. Device programming depends on the exact controller, tool and storage."
aside: false
journey: network
---

<!-- Generated from site/.vitepress/theme/journey-data.ts by site/scripts/journey-pages.mjs. Edit the metadata owner; review --json output and apply it explicitly. -->

# NIC / MAC guide chooser

Choose a Windows override or your NIC. Device programming depends on the exact controller, tool and storage.

- **Identify:** [Current, permanent and stored MAC](../../guides/mac-spoofing/mac-spoofing.md#current-mac-permanent-mac-and-burned-in-storage)
- **Prepare:** [Backups and recovery checklist](../../guides/getting-started/getting-started.md#safety-checklist)
- **Full guide:** [NIC and MAC addresses](../../guides/mac-spoofing/mac-spoofing.md)
- **Verify:** [Verify current and device readbacks](../../guides/mac-spoofing/mac-spoofing.md#verification-checklist)

## Choose your route

```mermaid
%% hwid-journey:network
flowchart TD
  accTitle: MAC address route
  accDescr: Choose a Windows override or your NIC. Device programming depends on the exact controller, tool and storage.
  j_network_start(["MAC address"])
  j_network_connection{"Windows setting or adapter?"}
  j_network_internal{"Which onboard / PCIe NIC?"}
  j_network_usb{"Which USB adapter?"}
  j_network_network_windows["Windows MAC override\nSoftware only"]
  class j_network_network_windows journey-procedure
  j_network_network_intel["Intel EEUPDATE\nExact controller · Untested"]
  class j_network_network_intel journey-procedure
  j_network_network_realtek_pcie["Realtek PG\nExact CFG · Untested"]
  class j_network_network_realtek_pcie journey-procedure
  j_network_network_realtek_usb["Realtek USB PG\nSingle RTL8153 report"]
  class j_network_network_realtek_usb journey-research
  j_network_network_tplink_ue300["TP-Link UE300\nSingle report · check revision"]
  class j_network_network_tplink_ue300 journey-research
  j_network_network_asix_original["AX88179: ASIXFlash / Captain\nExact tool / storage"]
  class j_network_network_asix_original journey-research
  j_network_network_asix_ab["AX88179A / B\nExact tool / storage"]
  class j_network_network_asix_ab journey-background
  j_network_network_connectx3["CX311A: WinMFT flint\nExact model"]
  class j_network_network_connectx3 journey-procedure
  j_network_network_storage_limits["RTL8126 / AQC113 / unknown\nLimits"]
  class j_network_network_storage_limits journey-limitation
  j_network_start --> j_network_connection
  j_network_connection --> j_network_network_windows
  j_network_connection -->|"Onboard / PCIe NIC"| j_network_internal
  j_network_connection -->|"USB NIC"| j_network_usb
  j_network_connection -->|"Not sure"| j_network_network_storage_limits
  j_network_internal --> j_network_network_intel
  j_network_internal -->|"Realtek (not RTL8126)"| j_network_network_realtek_pcie
  j_network_internal --> j_network_network_connectx3
  j_network_internal --> j_network_network_storage_limits
  j_network_usb --> j_network_network_tplink_ue300
  j_network_usb -->|"Realtek RTL8153 / RTL8156"| j_network_network_realtek_usb
  j_network_usb --> j_network_network_asix_original
  j_network_usb --> j_network_network_asix_ab
  j_network_usb -->|"Other / not sure"| j_network_network_storage_limits
  click j_network_network_windows href "https://hwid.idkzal.cc/guides/mac-spoofing/mac-spoofing.html#windows-networkaddress-override-software-only" "Software override" _self
  click j_network_network_intel href "https://hwid.idkzal.cc/guides/mac-spoofing/mac-spoofing.html#intel-nics" "Intel EEUPDATE" _self
  click j_network_network_realtek_pcie href "https://hwid.idkzal.cc/guides/mac-spoofing/mac-spoofing.html#realtek-nics" "Realtek PG" _self
  click j_network_network_realtek_usb href "https://hwid.idkzal.cc/guides/mac-spoofing/mac-spoofing.html#realtek-usb-nics-update" "RTL8153 / RTL8156" _self
  click j_network_network_tplink_ue300 href "https://hwid.idkzal.cc/guides/mac-spoofing/mac-spoofing.html#tp-link-ue300--rtl8153" "TP-Link UE300" _self
  click j_network_network_asix_original href "https://hwid.idkzal.cc/guides/mac-spoofing/mac-spoofing.html#asix-ax88179ab-now-too" "ASIX AX88179" _self
  click j_network_network_asix_ab href "https://hwid.idkzal.cc/guides/mac-spoofing/mac-spoofing.html#ax88179-storage-and-captain-tool-detail" "AX88179A / B" _self
  click j_network_network_connectx3 href "https://hwid.idkzal.cc/guides/mac-spoofing/mac-spoofing.html#mellanox-connectx-3-cx311a--mcx311a-xcat" "ConnectX-3" _self
  click j_network_network_storage_limits href "https://hwid.idkzal.cc/guides/mac-spoofing/mac-spoofing.html#controller-storage-efuse-eeprom-or-flash" "Controller storage #amp; limits" _self
```

## Windows and address layers

- [Address layers](../../guides/mac-spoofing/mac-spoofing.md#current-mac-permanent-mac-and-burned-in-storage) (background). **Current, permanent and stored addresses are separate views**
- [Software override](../../guides/mac-spoofing/mac-spoofing.md#windows-networkaddress-override-software-only) (procedure). **Documented Windows mechanism; driver support varies** Changes the current address without rewriting NIC storage; applying it restarts the adapter.
  Topic names: Windows NetworkAddress: software-only override.

## Internal / PCIe

- [Intel EEUPDATE](../../guides/mac-spoofing/mac-spoofing.md#intel-nics) (procedure). **General programming procedure untested** Exact controller, utility version, locks and backup caveats apply.
  Topic names: Intel Ethernet: EEUPDATE / DOS route.
- [Realtek PG](../../guides/mac-spoofing/mac-spoofing.md#realtek-nics) (procedure). **General workflow untested; exact silicon / configuration required** RTL8125-family configuration does not establish RTL8126 support.
  Topic names: Realtek onboard / PCIe: matched PG configuration.
- [ConnectX-3](../../guides/mac-spoofing/mac-spoofing.md#mellanox-connectx-3-cx311a--mcx311a-xcat) (procedure). **Named CX311A single-port hardware test** Read WinOF / WinMFT Prerequisites, firmware backup and image-file checks before flashing.
  Topic names: Mellanox ConnectX-3 CX311A / MCX311A-XCAT.

## USB adapters

- [RTL8153 / RTL8156](../../guides/mac-spoofing/mac-spoofing.md#realtek-usb-nics-update) (research). **Named Belkin RTL8153 contributor report; not repeated here** The tool-version and OTP/eFuse limits remain explicit; do not extend the report to every RTL8156 adapter.
  Topic names: Realtek RTL8153 / RTL8156 USB adapters.
- [TP-Link UE300](../../guides/mac-spoofing/mac-spoofing.md#tp-link-ue300--rtl8153) (research). **One report; revision and active storage unconfirmed** Record the printed revision and matched hardware before considering its cautious Realtek workflow.
  Topic names: TP-Link UE300 / RTL8153: revision-sensitive report.
- [ASIX AX88179](../../guides/mac-spoofing/mac-spoofing.md#asix-ax88179ab-now-too) (research). **Third-party Captain report; exact controller / storage required** External EEPROM and embedded eFuse are different storage paths.
  Topic names: ASIX AX88179: original-controller route.
- [AX88179A / B](../../guides/mac-spoofing/mac-spoofing.md#ax88179-storage-and-captain-tool-detail) (background). **Vendor storage model; exact tooling / adapter support required** A/B bundled-tool limits are not a vendor-wide rule. eFuse cannot be erased.
  Topic names: ASIX AX88179A / AX88179B: revision and storage.

## RTL8126 / AQC113 / other

- [Controller storage & limits](../../guides/mac-spoofing/mac-spoofing.md#controller-storage-efuse-eeprom-or-flash) (limitation). **RTL8126 factory provisioning / AQC113 recovery / unknown storage** One shared section. Exact controller/card, adapter revision, hardware IDs, tool version and storage mode are needed; no generic detection commands are added.
  Topic names: Realtek RTL8126: factory provisioning boundary; Marvell / Aquantia AQC113: recovery, not MAC editing; Unknown NIC or unknown storage mode.

[Home](../home.md) · [Complete work order](../start.md) · [All hardware topics](../devices.md) · [Reference](../reference.md)
