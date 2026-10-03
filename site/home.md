---
layout: home
title: HWID Privacy

hero:
  name: HWID Privacy
  text: Take your hardware identity back
  tagline: Find which parts of your PC give it a fixed identity, and change them one by one with tested steps.
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
    details: "SMBIOS system, board and chassis fields. Risk: high. Difficulty: medium."
    link: /guides/motherboard-spoofing/motherboard-spoofing
  - title: NVRAM
    details: "Read-only inventory of EFI variables. Risk: low. Difficulty: easy."
    link: /guides/nvram-spoofing/nvram-spoofing
  - title: TPM
    details: "dTPM swap and TPM clear. Risk: medium. Difficulty: medium."
    link: /guides/tpm-spoofing/tpm-spoofing
  - title: fTPM reset (AM5)
    details: "AMD firmware TPM identity reset reports. Risk: medium. Difficulty: medium."
    link: /guides/resets/ftpm-reset-tutorial
  - title: Storage
    details: "SSD model, serial and firmware on supported controllers. Risk: high. Difficulty: hard."
    link: /guides/ssd-spoofing/ssd-spoofing
  - title: MAC address
    details: "Software override up to controller eFuse writes. Risk: low to high. Difficulty: easy to hard."
    link: /guides/mac-spoofing/mac-spoofing
  - title: Router
    details: "Your own router as the first hop the PC sees. Risk: low. Difficulty: medium."
    link: /guides/arp-spoofing/arp-spoofing
  - title: RAM
    details: "SPD serial with an external programmer. Risk: high. Difficulty: hard."
    link: /guides/ram-spoofing/ram-spoofing
  - title: Monitor
    details: "EDID identity through an emulator. Risk: medium. Difficulty: medium."
    link: /guides/monitor-spoofing/monitor-spoofing
---
