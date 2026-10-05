---
title: Start here
description: The full work order, with links to the hardware privacy guides, preparation and verification.
aside: false
pageClass: wiki-start
---

# Start here

Use the flowchart to plan one component or a wider project. Click a linked box to open its guide or checklist. The full steps, warnings and sources stay in the detailed guides.

## Guide map

<!-- start-overview:begin -->
```mermaid
%% hwid-journey:start
flowchart TD
  accTitle: Overall guide map
  accDescr: Save your current IDs, prepare, choose a part and check the result.
  j_start_start(["Start here"])
  j_start_inspect["HWIDChecker\nRead-only inspection"]
  class j_start_inspect journey-identification
  j_start_nvram["EFI variables\nRead-only"]
  class j_start_nvram journey-identification
  j_start_stage_2["Save current IDs"]
  class j_start_stage_2 journey-preparation
  j_start_stage_3["Backup and recovery"]
  class j_start_stage_3 journey-preparation
  j_start_stage_4["Check device limits"]
  class j_start_stage_4 journey-preparation
  j_start_component{"Which part?"}
  j_start_motherboard["Motherboard"]
  class j_start_motherboard journey-background
  j_start_tpm["TPM"]
  class j_start_tpm journey-background
  j_start_ftpm["fTPM"]
  class j_start_ftpm journey-background
  j_start_storage["SSD / storage"]
  class j_start_storage journey-background
  j_start_network["MAC address"]
  class j_start_network journey-background
  j_start_memory["RAM"]
  class j_start_memory journey-background
  j_start_display["Monitor"]
  class j_start_display journey-background
  j_start_router["Router"]
  class j_start_router journey-background
  j_start_stage_8["Verification by part"]
  class j_start_stage_8 journey-verification
  j_start_stage_10["Save after IDs"]
  class j_start_stage_10 journey-verification
  j_start_stage_9["Reinstall Windows\nOptional"]
  class j_start_stage_9 journey-procedure
  j_start_start -->|"View IDs"| j_start_inspect
  j_start_start -->|"Inspect EFI"| j_start_nvram
  j_start_start -->|"Change IDs"| j_start_stage_2
  j_start_stage_2 --> j_start_stage_3
  j_start_stage_3 --> j_start_stage_4
  j_start_stage_4 --> j_start_component
  j_start_component --> j_start_motherboard
  j_start_motherboard --> j_start_stage_8
  j_start_component --> j_start_tpm
  j_start_tpm --> j_start_stage_8
  j_start_component --> j_start_ftpm
  j_start_ftpm --> j_start_stage_8
  j_start_component --> j_start_storage
  j_start_storage --> j_start_stage_8
  j_start_component --> j_start_network
  j_start_network --> j_start_stage_8
  j_start_component --> j_start_memory
  j_start_memory --> j_start_stage_8
  j_start_component --> j_start_display
  j_start_display --> j_start_stage_8
  j_start_component --> j_start_router
  j_start_router --> j_start_stage_8
  j_start_stage_8 -->|"Another part"| j_start_component
  j_start_stage_8 --> j_start_stage_10
  j_start_stage_8 -->|"If planned"| j_start_stage_9
  j_start_stage_9 --> j_start_stage_10
  click j_start_inspect href "https://hwid.idkzal.cc/guides/getting-started/getting-started.html#hwidcheckerexe" "View IDs with HWIDChecker" _self
  click j_start_nvram href "https://hwid.idkzal.cc/hardware/nvram.html" "Inspect EFI variables" _self
  click j_start_stage_2 href "https://hwid.idkzal.cc/guides/getting-started/getting-started.html#take-before-and-after-snapshots" "Save current IDs" _self
  click j_start_stage_3 href "https://hwid.idkzal.cc/guides/getting-started/getting-started.html#safety-checklist" "Backup and recovery" _self
  click j_start_stage_4 href "https://hwid.idkzal.cc/guides/getting-started/getting-started.html#device-restrictions" "Check device limits" _self
  click j_start_motherboard href "https://hwid.idkzal.cc/hardware/motherboard.html" "Motherboard guide" _self
  click j_start_tpm href "https://hwid.idkzal.cc/hardware/tpm.html" "TPM guide" _self
  click j_start_ftpm href "https://hwid.idkzal.cc/hardware/ftpm.html" "fTPM evidence guide" _self
  click j_start_storage href "https://hwid.idkzal.cc/hardware/storage.html" "Storage guide" _self
  click j_start_network href "https://hwid.idkzal.cc/hardware/network.html" "NIC / MAC guide" _self
  click j_start_memory href "https://hwid.idkzal.cc/hardware/memory.html" "RAM / SPD guide" _self
  click j_start_display href "https://hwid.idkzal.cc/hardware/display.html" "Monitor / EDID guide" _self
  click j_start_router href "https://hwid.idkzal.cc/hardware/router.html" "Router / gateway guide" _self
  click j_start_stage_8 href "https://hwid.idkzal.cc/start.html#stage-8" "Verification links by component" _self
  click j_start_stage_10 href "https://hwid.idkzal.cc/guides/getting-started/getting-started.html#take-before-and-after-snapshots" "Save after IDs" _self
  click j_start_stage_9 href "https://hwid.idkzal.cc/guides/getting-started/getting-started.html#clean-windows-reinstall-checklist" "Clean Windows installation" _self
```
<!-- start-overview:end -->

For several components, follow the [work order below](#work-order). Use each component guide's preparation and verification checks.

## Choose how to begin

- [Learn what an HWID is](/guides/getting-started/getting-started.html#what-an-hwid-is)
- [See the IDs on your PC with HWIDChecker](/guides/getting-started/getting-started.html#hwidcheckerexe)
- [Go straight to a component](/devices.html)

## Work order

> [!WARNING]
> This sequence is a project workflow, not a universal vendor procedure. It has not been validated on every platform and is **[S]**. Any firmware or device write keeps the evidence grade and warning from its dedicated guide. Do not use this summary as a write procedure.

<WorkflowMap />

[Read the full step list](/guides/getting-started/getting-started.html#plan-the-work-in-the-right-order)

The full Getting Started guide also covers [identifier groups](/guides/getting-started/getting-started.html#identifier-groups), [device restrictions](/guides/getting-started/getting-started.html#device-restrictions) and [recovery preparation](/guides/getting-started/getting-started.html#safety-checklist).
