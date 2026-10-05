---
title: "TPM guide chooser"
description: "Identify the active implementation and distinguish protected state, endorsement keys and certificates."
aside: false
journey: tpm
---

<!-- Generated from site/.vitepress/theme/journey-data.ts by site/scripts/journey-pages.mjs. Edit the metadata owner; review --json output and apply it explicitly. -->

# TPM guide chooser

Identify the active implementation and distinguish protected state, endorsement keys and certificates.

- **Identify:** [Identify the TPM implementation](../../guides/tpm-spoofing/tpm-spoofing.md#tpm-implementation-types)
- **Prepare:** [TPM key and sign-in precautions](../../guides/tpm-spoofing/tpm-spoofing.md#what-clearing-the-tpm-changes)
- **Full guide:** [TPM identity and implementation](../../guides/tpm-spoofing/tpm-spoofing.md)
- **Verify:** [Verify the EK and certificate collections](../../guides/tpm-spoofing/tpm-spoofing.md#verify-with-hwidcheckerexe)

## Choose your route

```mermaid
%% hwid-journey:tpm
flowchart TD
  accTitle: TPM identity and implementation route
  accDescr: Identify the active implementation and distinguish protected state, endorsement keys and certificates.
  j_tpm_start(["TPM identity"])
  j_tpm_technology{"Which TPM topic?"}
  j_tpm_tpm_clear["TPM clear\nRead effects"]
  class j_tpm_tpm_clear journey-limitation
  j_tpm_tpm_update_continuity["Firmware update\nEK continuity"]
  class j_tpm_tpm_update_continuity journey-background
  j_tpm_tpm_amd["AMD fTPM observations\nReports"]
  class j_tpm_tpm_amd journey-background
  j_tpm_tpm_intel["Intel PTT reports\nLimits"]
  class j_tpm_tpm_intel journey-limitation
  j_tpm_tpm_types["Identify TPM type"]
  class j_tpm_tpm_types journey-identification
  j_tpm_start --> j_tpm_technology
  j_tpm_technology --> j_tpm_tpm_amd
  j_tpm_technology --> j_tpm_tpm_intel
  j_tpm_technology --> j_tpm_tpm_types
  j_tpm_technology --> j_tpm_tpm_clear
  j_tpm_technology --> j_tpm_tpm_update_continuity
  click j_tpm_tpm_clear href "https://hwid.idkzal.cc/guides/tpm-spoofing/tpm-spoofing.html#what-clearing-the-tpm-changes" "Clear limits" _self
  click j_tpm_tpm_update_continuity href "https://hwid.idkzal.cc/guides/tpm-spoofing/tpm-spoofing.html#firmware-updates-and-ek-continuity" "Update continuity" _self
  click j_tpm_tpm_amd href "https://hwid.idkzal.cc/guides/resets/ftpm-reset-tutorial.html#how-the-amd-ftpm-identity-actually-works" "AMD fTPM evidence" _self
  click j_tpm_tpm_intel href "https://hwid.idkzal.cc/guides/resets/ftpm-reset-tutorial.html#intel-z790-vs-z790-era-method-vs-z890" "Intel generation limits" _self
  click j_tpm_tpm_types href "https://hwid.idkzal.cc/guides/tpm-spoofing/tpm-spoofing.html#tpm-implementation-types" "dTPM / Pluton types" _self
```

## Implementation

- [dTPM / Pluton types](../../guides/tpm-spoofing/tpm-spoofing.md#tpm-implementation-types) (identification). **Implementation table, not separate rotation procedures** Confirm the active implementation in firmware or device documentation.
  Topic names: Discrete TPM chip or plug-in module; Microsoft Pluton configured as TPM.

## State & identity

- [Clear limits](../../guides/tpm-spoofing/tpm-spoofing.md#what-clearing-the-tpm-changes) (limitation). **Standard clear changes storage state, not EPS / default EK** Read the key, recovery and alternate sign-in precautions.
  Topic names: Ordinary TPM clear: state, not EK rotation.
- [Update continuity](../../guides/tpm-spoofing/tpm-spoofing.md#firmware-updates-and-ek-continuity) (background). **Firmware-version change alone does not prove a new standard EK**
  Topic names: Firmware update and EK continuity.

## Related fTPM chapter

- [AMD fTPM evidence](../../guides/resets/ftpm-reset-tutorial.md#how-the-amd-ftpm-identity-actually-works) (background). **AMD observations and derivation limits; fTPM chapter** Opens the fTPM guide. Exact derivation inputs remain undocumented.
  Topic names: AMD ASP fTPM.
- [Intel generation limits](../../guides/resets/ftpm-reset-tutorial.md#intel-z790-vs-z790-era-method-vs-z890) (limitation). **Generation-specific evidence; fTPM chapter** Shared canonical section with ftpm-intel-z890. Do not generalize the MSI Z790 report.
  Topic names: Intel Platform Trust Technology (PTT).

[Home](../home.md) · [Complete work order](../start.md) · [All hardware topics](../devices.md) · [Reference](../reference.md)
