---
title: "Monitor / EDID guide chooser"
description: "Choose Windows settings, an HDMI emulator or the monitor EEPROM. Keep the full EDID and signal features."
aside: false
journey: display
---

<!-- Generated from site/.vitepress/theme/journey-data.ts by site/scripts/journey-pages.mjs. Edit the metadata owner; review --json output and apply it explicitly. -->

# Monitor / EDID guide chooser

Choose Windows settings, an HDMI emulator or the monitor EEPROM. Keep the full EDID and signal features.

- **Identify:** [Identify EDID and the connection path](../../guides/monitor-spoofing/monitor-spoofing.md#how-monitor-identity-is-stored)
- **Prepare:** [Backup and recovery requirements](../../guides/monitor-spoofing/monitor-spoofing.md#requirements)
- **Full guide:** [Monitor and EDID identity](../../guides/monitor-spoofing/monitor-spoofing.md)
- **Verify:** [Verify identity and display features](../../guides/monitor-spoofing/monitor-spoofing.md#verify-with-hwidchecker)

## Choose your route

```mermaid
%% hwid-journey:display
flowchart TD
  accTitle: Monitor and EDID route
  accDescr: Choose Windows settings, an HDMI emulator or the monitor EEPROM. Keep the full EDID and signal features.
  j_display_start(["Monitor / EDID"])
  j_display_scope{"Which EDID method?"}
  j_display_device{"Which HDMI emulator?"}
  j_display_identity["Check monitor / input"]
  class j_display_identity journey-identification
  j_display_display_windows["CRU / monitor INF\nWindows only"]
  class j_display_display_windows journey-procedure
  j_display_display_drhdmi4k["Dr HDMI 4K\n256-byte EDID"]
  class j_display_display_drhdmi4k journey-procedure
  j_display_display_drhdmi8k["Dr HDMI 8K\nCheck firmware / capacity"]
  class j_display_display_drhdmi8k journey-procedure
  j_display_display_generic["Other HDMI adapter\nProgramming unverified"]
  class j_display_display_generic journey-limitation
  j_display_display_dichen["Dichen 5\nUntested"]
  class j_display_display_dichen journey-research
  j_display_display_eeprom["VG248QE EEPROM\n2013 report · untested here"]
  class j_display_display_eeprom journey-research
  j_display_display_displayport["Native DisplayPort / DDC\nHDMI-only limit"]
  class j_display_display_displayport journey-limitation
  j_display_start --> j_display_scope
  j_display_scope --> j_display_display_windows
  j_display_scope -->|"HDMI emulator"| j_display_device
  j_display_scope --> j_display_display_eeprom
  j_display_scope --> j_display_display_displayport
  j_display_scope -->|"Input / method unknown"| j_display_identity
  j_display_device --> j_display_display_drhdmi4k
  j_display_device --> j_display_display_drhdmi8k
  j_display_device --> j_display_display_dichen
  j_display_device --> j_display_display_generic
  click j_display_identity href "https://hwid.idkzal.cc/guides/monitor-spoofing/monitor-spoofing.html#how-monitor-identity-is-stored" "Identify EDID / input" _self
  click j_display_display_windows href "https://hwid.idkzal.cc/guides/monitor-spoofing/monitor-spoofing.html#option-1-windows-software-override" "CRU / INF override" _self
  click j_display_display_drhdmi4k href "https://hwid.idkzal.cc/guides/monitor-spoofing/monitor-spoofing.html#dr-hdmi-4k" "Dr HDMI 4K" _self
  click j_display_display_drhdmi8k href "https://hwid.idkzal.cc/guides/monitor-spoofing/monitor-spoofing.html#dr-hdmi-8k" "Dr HDMI 8K" _self
  click j_display_display_generic href "https://hwid.idkzal.cc/guides/monitor-spoofing/monitor-spoofing.html#generic-hdmi-edid-adapters" "Generic adapter limits" _self
  click j_display_display_dichen href "https://hwid.idkzal.cc/guides/monitor-spoofing/monitor-spoofing.html#dichen-5-programmable-fuser" "Dichen research" _self
  click j_display_display_eeprom href "https://hwid.idkzal.cc/guides/monitor-spoofing/monitor-spoofing.html#option-4-direct-monitor-eeprom-modification" "Direct EEPROM research" _self
  click j_display_display_displayport href "https://hwid.idkzal.cc/guides/monitor-spoofing/monitor-spoofing.html#ddc-ddcci-hdmi-and-displayport" "DisplayPort / DDC limits" _self
```

## Identify & prepare

- [Identify EDID / input](../../guides/monitor-spoofing/monitor-spoofing.md#how-monitor-identity-is-stored) (identification). **Complete descriptor and exact connection-path scope**
- [Prepare EDID](../../guides/monitor-spoofing/monitor-spoofing.md#prepare-a-safe-edited-edid) (preparation). **Capture, edit minimally and validate every descriptor block**

## Windows

- [CRU / INF override](../../guides/monitor-spoofing/monitor-spoofing.md#option-1-windows-software-override) (procedure). **Documented Windows override; driver / path support varies** Does not rewrite monitor hardware.
  Topic names: Windows EDID override: CRU / monitor INF.

## Inline HDMI

- [Dr HDMI 4K](../../guides/monitor-spoofing/monitor-spoofing.md#dr-hdmi-4k) (procedure). **Vendor mechanism; 256-byte capacity and signal fit required** Do not truncate a larger EDID to fit the device.
  Topic names: Dr HDMI 4K: programmable inline EDID.
- [Dr HDMI 8K](../../guides/monitor-spoofing/monitor-spoofing.md#dr-hdmi-8k) (procedure). **Vendor mechanism; firmware and complete EDID capacity required** Firmware 1.4 adds documented 384 / 512-byte support. Signal-feature support is a separate device requirement.
  Topic names: Dr HDMI 8K: firmware-dependent extended EDID.
- [Generic adapter limits](../../guides/monitor-spoofing/monitor-spoofing.md#generic-hdmi-edid-adapters) (limitation). **No generic programming model or software verified** Presets or sink copy do not establish user-programmable EDID.
  Topic names: Generic HDMI adapter: programming support unknown.
- [Dichen research](../../guides/monitor-spoofing/monitor-spoofing.md#dichen-5-programmable-fuser) (research). **Dichen-specific actions remain untested** Authoritative capacity, utility and recovery evidence is missing.
  Topic names: Dichen 5 / DC240HZ5D-2: untested research.

## Other paths

- [Direct EEPROM research](../../guides/monitor-spoofing/monitor-spoofing.md#option-4-direct-monitor-eeprom-modification) (research). **ASUS VG248QE 2013 third-party report; untested here** Does not establish another board revision, monitor model or DisplayPort path.
  Topic names: Direct EEPROM: exact monitor / input only.
- [DisplayPort / DDC limits](../../guides/monitor-spoofing/monitor-spoofing.md#ddc-ddcci-hdmi-and-displayport) (limitation). **HDMI-only emulation does not intercept native DisplayPort** Working DDC/CI controls do not establish EEPROM write access.
  Topic names: DisplayPort / DDC path: hardware boundary.

[Home](../home.md) · [Complete work order](../start.md) · [All hardware topics](../devices.md) · [Reference](../reference.md)
