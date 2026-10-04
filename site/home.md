---
layout: home
title: HWID Privacy

hero:
  name: HWID Privacy
  text: Hardware ID guides for Windows PCs
  tagline: Which parts of a PC have a fixed ID, and how to change them.
  actions:
    - theme: brand
      text: Get started
      link: /guides/getting-started/getting-started
    - theme: alt
      text: Download HWIDChecker
      link: https://github.com/Fundryi/HWID-Privacy/raw/main/HWIDChecker.exe

# Risk and difficulty come from the warning boxes in each guide. Refresh after guide edits.
features:
  - title: Motherboard
    details: "SMBIOS system, board and chassis fields.<span class=\"chips\"><span class=\"chip high\">Risk: high</span><span class=\"chip\">Difficulty: medium</span></span>"
    link: /guides/motherboard-spoofing/motherboard-spoofing
  - title: NVRAM
    details: "Read-only inventory of EFI variables.<span class=\"chips\"><span class=\"chip low\">Risk: low</span><span class=\"chip\">Difficulty: easy</span></span>"
    link: /guides/nvram-spoofing/nvram-spoofing
  - title: TPM
    details: "dTPM swap and TPM clear.<span class=\"chips\"><span class=\"chip medium\">Risk: medium</span><span class=\"chip\">Difficulty: medium</span></span>"
    link: /guides/tpm-spoofing/tpm-spoofing
  - title: fTPM reset (AM5)
    details: "AMD firmware TPM identity reset reports.<span class=\"chips\"><span class=\"chip medium\">Risk: medium</span><span class=\"chip\">Difficulty: medium</span></span>"
    link: /guides/resets/ftpm-reset-tutorial
  - title: Storage
    details: "SSD model, serial and firmware on supported controllers.<span class=\"chips\"><span class=\"chip high\">Risk: high</span><span class=\"chip\">Difficulty: hard</span></span>"
    link: /guides/ssd-spoofing/ssd-spoofing
  - title: MAC address
    details: "Software override up to controller eFuse writes.<span class=\"chips\"><span class=\"chip medium\">Risk: low to high</span><span class=\"chip\">Difficulty: easy to hard</span></span>"
    link: /guides/mac-spoofing/mac-spoofing
  - title: Router
    details: "Your own router as the first hop the PC sees.<span class=\"chips\"><span class=\"chip low\">Risk: low</span><span class=\"chip\">Difficulty: medium</span></span>"
    link: /guides/arp-spoofing/arp-spoofing
  - title: RAM
    details: "SPD serial with an external programmer.<span class=\"chips\"><span class=\"chip high\">Risk: high</span><span class=\"chip\">Difficulty: hard</span></span>"
    link: /guides/ram-spoofing/ram-spoofing
  - title: Monitor
    details: "EDID identity through an emulator.<span class=\"chips\"><span class=\"chip medium\">Risk: medium</span><span class=\"chip\">Difficulty: medium</span></span>"
    link: /guides/monitor-spoofing/monitor-spoofing
---

## HWIDChecker

![HWIDChecker main window, Mask IDs on: serials, MACs and GUIDs show as X](./screenshots/main-masked.png)

Native Windows 10/11 app, no runtime needed, administrator rights on every launch. It lists the identifiers from every guide in one window, exports them as text, and compares an export with the current system.

![Compare files: before and after export side by side. Green: identifier changed. Red: unique identifier still the same.](./screenshots/compare.png)

[Download HWIDChecker.exe](https://github.com/Fundryi/HWID-Privacy/raw/main/HWIDChecker.exe) · [How to take before and after snapshots](/guides/getting-started/getting-started#hwidcheckerexe)
