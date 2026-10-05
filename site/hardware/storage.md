---
title: "Storage guide chooser"
description: "Choose your SSD, enclosure or volume. Match the controller, NAND, revision and connection before SSD work."
aside: false
journey: storage
---

<!-- Generated from site/.vitepress/theme/journey-data.ts by site/scripts/journey-pages.mjs. Edit the metadata owner; review --json output and apply it explicitly. -->

# Storage guide chooser

Choose your SSD, enclosure or volume. Match the controller, NAND, revision and connection before SSD work.

- **Identify:** [Identify the exact controller](../../guides/ssd-spoofing/ssd-spoofing.md#which-controller-do-i-have)
- **Prepare:** [Backups and recovery checklist](../../guides/getting-started/getting-started.md#safety-checklist)
- **Full guide:** [SSD, storage controllers and USB bridges](../../guides/ssd-spoofing/ssd-spoofing.md)
- **Verify:** [Verify every storage layer](../../guides/ssd-spoofing/ssd-spoofing.md#verify-the-result)

## Choose your route

```mermaid
%% hwid-journey:storage
flowchart TD
  accTitle: Storage identity route
  accDescr: Choose your SSD, enclosure or volume. Match the controller, NAND, revision and connection before SSD work.
  j_storage_start(["Storage identity"])
  j_storage_layer{"SSD, USB enclosure or volume?"}
  j_storage_controller{"Which SSD / controller?"}
  j_storage_storage_map1202["MAP1202: MXMPTool\nExact controller + NAND"]
  class j_storage_storage_map1202 journey-procedure
  j_storage_storage_yansen_kingspec["YANSEN / KingSpec: SSDToolKits\nExact tested setup"]
  class j_storage_storage_yansen_kingspec journey-procedure
  j_storage_storage_sm2263xt["SM2263XT MP tool\nUntested"]
  class j_storage_storage_sm2263xt journey-research
  j_storage_storage_bridges["RTL9210B / TUSB926x\nBridge only · RTL untested"]
  class j_storage_storage_bridges journey-research
  j_storage_storage_raid_volume["RAID / partitions / volumes"]
  class j_storage_storage_raid_volume journey-background
  j_storage_storage_research["MAP1602 / SM2269XT / IG5236\nResearch only"]
  class j_storage_storage_research journey-research
  j_storage_storage_unknown["Check controller + NAND"]
  class j_storage_storage_unknown journey-identification
  j_storage_start --> j_storage_layer
  j_storage_layer -->|"SSD"| j_storage_controller
  j_storage_layer -->|"USB enclosure"| j_storage_storage_bridges
  j_storage_layer --> j_storage_storage_raid_volume
  j_storage_layer -->|"Not sure"| j_storage_storage_unknown
  j_storage_controller --> j_storage_storage_map1202
  j_storage_controller -->|"ASMT 2115 bridge"| j_storage_storage_yansen_kingspec
  j_storage_controller --> j_storage_storage_sm2263xt
  j_storage_controller -->|"Other controller"| j_storage_storage_research
  j_storage_controller -->|"Not sure"| j_storage_storage_unknown
  click j_storage_storage_map1202 href "https://hwid.idkzal.cc/guides/ssd-spoofing/ssd-spoofing.html#m2-ssd-spoofing" "MAP1202" _self
  click j_storage_storage_yansen_kingspec href "https://hwid.idkzal.cc/guides/ssd-spoofing/ssd-spoofing.html#normal-25-ssd-spoofing" "YANSEN / KingSpec SATA" _self
  click j_storage_storage_sm2263xt href "https://hwid.idkzal.cc/guides/ssd-spoofing/ssd-spoofing.html#silicon-motion-sm2263xt-notes" "SM2263XT research" _self
  click j_storage_storage_bridges href "https://hwid.idkzal.cc/guides/ssd-spoofing/ssd-spoofing.html#usb-nvme-enclosures-and-bridge-serials" "RTL9210B / TUSB926x" _self
  click j_storage_storage_raid_volume href "https://hwid.idkzal.cc/guides/ssd-spoofing/ssd-spoofing.html#raid-disk-identity-and-volume-identity" "RAID / volumes" _self
  click j_storage_storage_research href "https://hwid.idkzal.cc/guides/ssd-spoofing/ssd-spoofing.html#research-candidates" "MAP1602 / SM2269XT / IG5236 / limits" _self
  click j_storage_storage_unknown href "https://hwid.idkzal.cc/guides/ssd-spoofing/ssd-spoofing.html#which-controller-do-i-have" "Identify controller" _self
```

## Native drive routes

- [MAP1202](../../guides/ssd-spoofing/ssd-spoofing.md#m2-ssd-spoofing) (procedure). **Owner-tested MAP1202 setup; not independently repeated** Read the destructive-operation warning and local Prerequisites. Match controller and NAND, not just the retail name.
  Topic names: Maxio MAP1202: M.2 NVMe owner-tested workflow.
- [YANSEN / KingSpec SATA](../../guides/ssd-spoofing/ssd-spoofing.md#normal-25-ssd-spoofing) (procedure). **Owner-tested drive / ASMT 2115 setup; other stock unverified** Read the SATA warning and Prerequisites; the retail listing does not establish a controller or NAND match.
  Topic names: YANSEN / KingSpec: 2.5-inch SATA workflow.
- [SM2263XT research](../../guides/ssd-spoofing/ssd-spoofing.md#silicon-motion-sm2263xt-notes) (research). **Controller-specific programming procedure untested** Controller, NAND and exact PCB revision must match the package.
  Topic names: Silicon Motion SM2263XT: untested MP workflow.

## Bridge / logical layers

- [RTL9210B / TUSB926x](../../guides/ssd-spoofing/ssd-spoofing.md#usb-nvme-enclosures-and-bridge-serials) (research). **RTL bridge workflow untested; TI descriptor example documented** One shared section. Neither changes the native SSD behind the bridge.
  Topic names: RTL9210B enclosure: USB / SCSI bridge identity; TI TUSB926x: documented bridge-descriptor example.
- [RAID / volumes](../../guides/ssd-spoofing/ssd-spoofing.md#raid-disk-identity-and-volume-identity) (background). **Logical identities are separate from native drive identity**
  Topic names: RAID, virtual disks, partitions and volumes.

## Other controllers

- [MAP1602 / SM2269XT / IG5236 / limits](../../guides/ssd-spoofing/ssd-spoofing.md#research-candidates) (research). **Bring-up / recovery evidence; no guide-supported identity change** Shared table for MAP1602, SM2269XT, IG5236, Realtek NVMe and Phison. No separate identity-write recipes.
  Topic names: Maxio MAP1602: bring-up research only; Silicon Motion SM2269XT: bring-up research only; InnoGrit IG5236: recovery evidence only; Realtek NVMe / Phison: unsupported identity change.

## Identify controller

- [Identify controller](../../guides/ssd-spoofing/ssd-spoofing.md#which-controller-do-i-have) (identification). **Exact controller, NAND, revision, firmware and transport required** Stop if the package is hidden or the match cannot be confirmed.
  Topic names: Unknown controller, NAND or bridge.

[Home](../home.md) · [Complete work order](../start.md) · [All hardware topics](../devices.md) · [Reference](../reference.md)
