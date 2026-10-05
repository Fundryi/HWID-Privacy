---
title: "Motherboard guide chooser"
description: "Identify the fields, exact firmware and recovery route before choosing board-specific work."
aside: false
journey: motherboard
---

<!-- Generated from site/.vitepress/theme/journey-data.ts by site/scripts/journey-pages.mjs. Edit the metadata owner; review --json output and apply it explicitly. -->

# Motherboard guide chooser

Identify the fields, exact firmware and recovery route before choosing board-specific work.

- **Identify:** [Identify the SMBIOS fields](../../guides/motherboard-spoofing/motherboard-spoofing.md#what-this-changes)
- **Prepare:** [Requirements and recovery](../../guides/motherboard-spoofing/motherboard-spoofing.md#requirements-and-recovery-preparation)
- **Full guide:** [Motherboard and SMBIOS](../../guides/motherboard-spoofing/motherboard-spoofing.md)
- **Verify:** [Verify SMBIOS after cold boot](../../guides/motherboard-spoofing/motherboard-spoofing.md#verify-with-hwidchecker)

## Choose your route

```mermaid
%% hwid-journey:motherboard
flowchart TD
  accTitle: Motherboard and SMBIOS route
  accDescr: Identify the fields, exact firmware and recovery route before choosing board-specific work.
  j_motherboard_start(["Motherboard / SMBIOS"])
  j_motherboard_firmware{"Firmware family?"}
  j_motherboard_motherboard_ami_dmiedit["AMI DMIEdit\nOwner report"]
  class j_motherboard_motherboard_ami_dmiedit journey-procedure
  j_motherboard_motherboard_insyde["Insyde H2OSDE\nResearch"]
  class j_motherboard_motherboard_insyde journey-research
  j_motherboard_motherboard_asus["ASUS ROM / FlashBack\nResearch"]
  class j_motherboard_motherboard_asus journey-research
  j_motherboard_motherboard_unknown["Board requirements\nIdentify"]
  class j_motherboard_motherboard_unknown journey-preparation
  j_motherboard_start --> j_motherboard_firmware
  j_motherboard_start -->|"ASUS ROM work"| j_motherboard_motherboard_asus
  j_motherboard_firmware --> j_motherboard_motherboard_ami_dmiedit
  j_motherboard_firmware --> j_motherboard_motherboard_insyde
  j_motherboard_firmware -->|"Other / unknown"| j_motherboard_motherboard_unknown
  click j_motherboard_motherboard_ami_dmiedit href "https://hwid.idkzal.cc/guides/motherboard-spoofing/motherboard-spoofing.html#instructions" "AMI DMIEdit" _self
  click j_motherboard_motherboard_insyde href "https://hwid.idkzal.cc/guides/motherboard-spoofing/motherboard-spoofing.html#insyde-h2osde-and-oem-provisioning-tools" "Insyde / H2OSDE" _self
  click j_motherboard_motherboard_asus href "https://hwid.idkzal.cc/guides/motherboard-spoofing/motherboard-spoofing.html#asus-specific-procedure-boundary" "ASUS ROM boundary" _self
  click j_motherboard_motherboard_unknown href "https://hwid.idkzal.cc/guides/motherboard-spoofing/motherboard-spoofing.html#requirements-and-recovery-preparation" "Identify #amp; recover" _self
```

## Identify & recover

- [SMBIOS fields](../../guides/motherboard-spoofing/motherboard-spoofing.md#what-this-changes) (background). **SMBIOS structure and field explanation**
- [Identify & recover](../../guides/motherboard-spoofing/motherboard-spoofing.md#requirements-and-recovery-preparation) (preparation). **Exact model, revision, BIOS and recovery are required** A rejected write or unsupported board remains a stop condition.
  Topic names: Unknown, protected or unsupported board.

## AMI tools

- [AMI DMIEdit](../../guides/motherboard-spoofing/motherboard-spoofing.md#instructions) (procedure). **Owner hardware report; other-board compatibility unverified** Read recovery preparation and review every command before writing.
  Topic names: AMI board: owner-tested DMIEdit workflow.
- [AMI scope & limits](../../guides/motherboard-spoofing/motherboard-spoofing.md#amidewin-and-dmiedit-workflow-notes) (background). **Command scope and compatibility limits** Supporting notes for the AMI workflow, not a second independent procedure.
  Topic names: AMIDEWIN / DMIEdit scope and limits.

## OEM / board research

- [Insyde / H2OSDE](../../guides/motherboard-spoofing/motherboard-spoofing.md#insyde-h2osde-and-oem-provisioning-tools) (research). **Tool existence documented; end-user availability unverified** Model-specific OEM provisioning; no general Phoenix procedure is confirmed.
  Topic names: Insyde H2OSDE and OEM provisioning.
- [ASUS ROM boundary](../../guides/motherboard-spoofing/motherboard-spoofing.md#asus-specific-procedure-boundary) (research). **Official recovery documented; modified-ROM procedure untested** A readable dump does not validate a modified image or a flash.
  Topic names: ASUS: FlashBack and modified-ROM boundary.

[Home](../home.md) · [Complete work order](../start.md) · [All hardware topics](../devices.md) · [Reference](../reference.md)
