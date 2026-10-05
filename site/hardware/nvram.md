---
title: "EFI variables guide chooser"
description: "Inspect variables read-only and keep boot configuration separate from reported identity-like names."
aside: false
journey: nvram
---

<!-- Generated from site/.vitepress/theme/journey-data.ts by site/scripts/journey-pages.mjs. Edit the metadata owner; review --json output and apply it explicitly. -->

# EFI variables guide chooser

Inspect variables read-only and keep boot configuration separate from reported identity-like names.

- **Identify:** [Understand variable names and roles](../../guides/nvram-spoofing/nvram-spoofing.md#what-the-variables-mean)
- **Prepare:** [Backup and recovery preparation](../../guides/nvram-spoofing/nvram-spoofing.md#backup-and-recovery-preparation)
- **Full guide:** [NVRAM and EFI variables](../../guides/nvram-spoofing/nvram-spoofing.md)
- **Verify:** [Compare EFI and SMBIOS evidence](../../guides/nvram-spoofing/nvram-spoofing.md#verify-with-hwidchecker)

## Choose your route

```mermaid
%% hwid-journey:nvram
flowchart TD
  accTitle: EFI-variable inspection route
  accDescr: Inspect variables read-only and keep boot configuration separate from reported identity-like names.
  j_nvram_start(["EFI variables / NVRAM"])
  j_nvram_uefi{"Live Linux booted via UEFI?"}
  j_nvram_nvram_read_only["List EFI variables\nRead-only"]
  class j_nvram_nvram_read_only journey-procedure
  j_nvram_requirements["UEFI requirements"]
  class j_nvram_requirements journey-preparation
  j_nvram_start --> j_nvram_uefi
  j_nvram_uefi -->|"Yes"| j_nvram_nvram_read_only
  j_nvram_uefi -->|"No / unknown"| j_nvram_requirements
  click j_nvram_nvram_read_only href "https://hwid.idkzal.cc/guides/nvram-spoofing/nvram-spoofing.html#read-only-inspection-steps" "UEFI inventory" _self
  click j_nvram_requirements href "https://hwid.idkzal.cc/guides/nvram-spoofing/nvram-spoofing.html#requirements" "Requirements" _self
```

## Inspect

- [Requirements](../../guides/nvram-spoofing/nvram-spoofing.md#requirements) (preparation). **Native UEFI and read-only evidence preparation**
- [UEFI inventory](../../guides/nvram-spoofing/nvram-spoofing.md#read-only-inspection-steps) (procedure). **Documented read-only inspection** HWIDChecker does not enumerate arbitrary EFI variables.
  Topic names: UEFI system: read-only efivarfs inventory.

## Interpret the inventory

- [Boot variables](../../guides/nvram-spoofing/nvram-spoofing.md#standard-boot-variables) (background). **Standard boot configuration, not motherboard serials**
  Topic names: BootOrder, Boot#### and other boot variables.
- [Reported ID names](../../guides/nvram-spoofing/nvram-spoofing.md#identifier-like-variables-reported-in-third-party-research) (research). **Third-party reported meanings remain unverified** A variable name does not establish its contents or purpose.
  Topic names: OfflineUniqueID, UnlockID, DmiVar and MacAddrVar.

## Write / delete limits

- [Write / delete limits](../../guides/nvram-spoofing/nvram-spoofing.md#why-this-guide-does-not-delete-variables) (limitation). **No verified generic deletion or restore procedure** Only an exact vendor-supported procedure could establish a supported change.
  Topic names: Unknown write or deletion: unsupported boundary.

[Home](../home.md) · [Complete work order](../start.md) · [All hardware topics](../devices.md) · [Reference](../reference.md)
