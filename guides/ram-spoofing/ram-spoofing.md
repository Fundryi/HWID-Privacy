# RAM Identifier Privacy Guide

RAM modules store configuration and manufacturing data in Serial Presence Detect (SPD) non-volatile memory. System firmware presents memory information through SMBIOS Memory Device structures, Type 17. On Windows, `Win32_PhysicalMemory` maps fields such as `SerialNumber` and `PartNumber` from SMBIOS. It is an SMBIOS view, not proof that an application read the SPD device directly. **[A]**

This guide covers DDR4 and DDR5 identity fields, read-only checks, and a conservative external-programmer workflow. The write and recovery procedures have not been tested by this project.

> [!CAUTION]
> A bad SPD write can prevent a module from completing memory initialization. Wrong organization, timing, voltage, or support-device data can also cause instability. Do not write to your only working module. Keep a verified full binary backup and a compatible external programmer before changing anything.

Evidence grades used here:

- **[C]** confirmed first-hand by a named user with hardware and procedure details
- **[A]** verified against a cited primary source
- **[CC]** supported by consistent reports from several independent users
- **[S]** a single unverified claim or an untested procedure

## Table of contents

- [Scope and data path](#scope-and-data-path)
- [Identity fields](#identity-fields)
- [DDR4 and DDR5 layout](#ddr4-and-ddr5-layout)
- [Write protection](#write-protection)
- [Lowest-risk options](#lowest-risk-options)
- [Requirements](#requirements)
- [Tools](#tools)
- [Read-only baseline](#read-only-baseline)
- [External-programmer procedure](#external-programmer-procedure)
- [Verify the result](#verify-the-result)
- [Recovery](#recovery)
- [Troubleshooting](#troubleshooting)
- [Sources](#sources)

## Scope and data path

JEDEC defines SPD contents so firmware can identify a module and initialize the memory channel. DMTF defines the SMBIOS Type 17 fields that firmware exposes to management software. Microsoft maps `Win32_PhysicalMemory.SerialNumber` and `PartNumber` from those Type 17 fields. The standards do not require firmware to expose every SPD byte or to preserve its raw formatting. **[A]**

The observable path on this project is:

1. The module stores SPD configuration and manufacturing data.
2. Firmware builds an SMBIOS Type 17 structure for each memory device.
3. Windows exposes those SMBIOS values through `Win32_PhysicalMemory`.
4. HWIDChecker displays `DeviceLocator`, `Manufacturer`, `PartNumber`, `Capacity`, and `SerialNumber` from that WMI class. **[A]**

PowerShell and HWIDChecker therefore show the same observation layer. Agreement between them is useful, but it is not an independent raw-SPD comparison.

The project [README RAM section](../../README.md#5-ram) lists Corsair, GeIL, and Trident Z G.Skill DDR4/DDR5 families as candidates that may report null serials. Treat the list as a starting point, not a guarantee for every model, production date, or PCB revision. Verify the exact modules before purchase or use.

## Identity fields

DDR4 and DDR5 define a manufacturing-information area. The main identity-related fields are below. **[A]**

| Field | Meaning | Privacy relevance |
|---|---|---|
| Module manufacturer ID | JEDEC JEP106 code for the module assembler | Identifies a vendor, not one module |
| Manufacturing location | Vendor-defined site code | Can narrow a production source |
| Manufacturing date | BCD year and week | Can narrow a production batch |
| Module serial number | Manufacturer-assigned four-byte value | Intended to identify one module |
| Module part number | Manufacturer-assigned ASCII product text | Identifies the model or family |
| Module revision | Vendor-defined assembly revision | Narrows the module variant |
| DRAM manufacturer ID | JEP106 code for the DRAM devices | Different from the module assembler |
| DRAM stepping | Vendor-defined DRAM revision | Narrows the DRAM revision |

Changing manufacturing fields does not change the physical DRAM devices, capacity, rank layout, bus width, timings, label, or barcode. It also does not change motherboard, CPU, storage, TPM, network, or USB identifiers. On DDR5 it does not change the identity or configuration of a separate PMIC, register clock driver, temperature sensor, or RGB controller. **[A]**

> [!NOTE]
> A changed SPD serial is one changed data point. It does not erase old inventory records or make a computer anonymous.

## DDR4 and DDR5 layout

### DDR4 manufacturing area

JEDEC Annex L defines a 512-byte DDR4 SPD address space. Manufacturing information occupies bytes 320 through 383, which are the upper half of 128-byte protection block 2. **[A]**

| DDR4 bytes | Contents |
|---|---|
| 320-321 | Module manufacturer ID |
| 322 | Manufacturing location |
| 323-324 | Manufacturing year and week |
| 325-328 | Module serial number |
| 329-348 | Module part number, 20 ASCII bytes |
| 349 | Module revision code |
| 350-351 | DRAM manufacturer ID |
| 352 | DRAM stepping |
| 353-381 | Module-manufacturer-specific data |
| 382-383 | Reserved, coded as `00h` |

DDR4 base-configuration CRC bytes are 126-127 and cover bytes 0-125. Module-specific CRC bytes are 254-255 and cover bytes 128-253. Neither CRC covers the manufacturing area at bytes 320-383. A serial-only change at bytes 325-328 must not change either CRC field. **[A]**

### DDR5 manufacturing area

DDR5 SPD5118 hubs provide 1024 bytes of non-volatile memory as sixteen 64-byte protection blocks. The manufacturing area is bytes 512 through 639, which are protection blocks 8 and 9. The serial and part number are both in block 8. **[A]**

| DDR5 bytes | Contents |
|---|---|
| 512-513 | Module manufacturer ID |
| 514 | Manufacturing location |
| 515-516 | Manufacturing year and week |
| 517-520 | Module serial number |
| 521-550 | Module part number, 30 ASCII bytes |
| 551 | Module revision code |
| 552-553 | DRAM manufacturer ID |
| 554 | DRAM stepping |
| 555-639 | Module-manufacturer-specific data |
| 640-1023 | End-user-programmable area |

DDR5 stores a CRC for bytes 0-509 in bytes 510-511. The manufacturing area begins at byte 512, so a serial-only change at bytes 517-520 does not require a CRC update. A tool that changes bytes 510-511 during a serial-only edit is not making the narrow change described by this guide. **[A]**

### Support devices on DDR5

An SPD5118 is a hub with SPD non-volatile memory and an optional temperature sensor. The hub also separates the host sideband bus from the local bus used by other module devices. A PMIC is a separate component that performs local voltage regulation and power sequencing. Some modules add a separate RGB controller. **[A]**

> [!CAUTION]
> Do not change PMIC registers, voltage settings, power sequences, timing fields, or overclocking profiles for identifier work. None is required for a serial-only edit.

## Write protection

### DDR4 EE1004 protection

Common DDR4 SPD devices implement the JEDEC EE1004 interface. The Microchip AT34C04 contains four independently settable reversible software write-protection regions of 128 bytes each. **[A]**

- The serial is in block 2, bytes 256-383. Inspect block 2 protection before doing anything else.
- JEDEC Annex L requires compliant suppliers to protect blocks 0 and 1. It requires block 2 protection when the extended-function area is used. It requires block 3 to remain unprotected. Block 2 is therefore not guaranteed to have the same protection state on every module. **[A]**
- On AT34C04, setting or clearing reversible software write protection requires the documented command while the `A0` pin is driven at the specified high voltage, `VHV`. **[A]**
- A clear command removes reversible protection from all four blocks at once. It cannot clear only block 2. Record every block's original state so it can be restored. **[A]**
- The AT34C04 does not implement legacy Permanent Software Write Protection. Do not send a `PSWP` command intended for another EEPROM generation or part. **[A]**

### DDR5 SPD5118 protection

SPD5118 hubs map protection for NVM blocks 0-7 in mode register `MR12` and blocks 8-15 in `MR13`. Bit 0 of `MR13` controls block 8, which contains serial bytes 517-520. **[A]**

In normal run-time mode, the hub allows a protection bit to be set to `1` but does not allow it to be cleared. The Renesas datasheet defines a separate offline-tester mode, selected by the documented `HSA` pin connection, in which protection bits may be cleared. Do not improvise that wiring. Use a programmer that explicitly implements SPD5118 offline-tester mode and reports the protection map. **[A]**

### Platform write disable

A writable SPD device can still be read-only from the operating system. Intel documents an `SPDWD` bit that blocks writes to the SPD SMBus address range until the next platform reset. The cited Core Ultra register uses on-wire addresses `A0h` through `AEh`, equivalent to 7-bit addresses `0x50` through `0x57`. BIOS is expected to set the lock. This is one documented Intel example, not a claim about every platform. **[A]**

A successful read therefore proves only read access. It does not prove that the chipset, firmware, protection state, and device will accept a write.

## Lowest-risk options

The lowest-risk choice is a module that already reports an empty or non-unique serial in the observation layer you care about. Verify it with the read-only checks below. No SPD write is then needed.

Replacing a module is safer than rewriting it. An external write is justified only when you can identify the exact SPD device, save a complete dump, verify support, and recover the module without booting it.

> [!WARNING]
> The external source reports that G.Skill modules with a `TA` prefix above the barcode were incompatible with one seller-specific programmer. This is a single unverified compatibility claim **[S]**. It does not establish compatibility with other programmers or other revisions.

## Requirements

Before any write, have all of the following:

- A second known-good bootable RAM module
- A programmer that explicitly supports the module's DDR generation and exact SPD device
- The programmer vendor's current wiring, voltage, connector, and protection documentation
- Two matching full reads of the untouched SPD
- A binary backup stored on two separate devices
- A way to restore the backup without booting the edited module
- An ESD-safe work area
- Time to stop and recover if the first readback differs

Kingston instructs users to disconnect power, prevent electrostatic discharge, handle a DIMM by its PCB corners, and avoid pressing on the integrated circuits. **[A]**

Do not continue if:

- The programmer documentation does not name the DDR generation and SPD hub or EEPROM.
- The software cannot save a complete 512-byte DDR4 or 1024-byte DDR5 raw image.
- Two reads of the untouched module differ.
- The tool reports an unknown device, incomplete dump, or bad existing CRC before editing.
- The target is your only working module or cannot be recovered externally.
- Access requires undocumented heat-spreader removal.

## Tools

| Tool | Purpose | Limits |
|---|---|---|
| `HWIDChecker.exe` in the repository root | Shows the Windows SMBIOS view used by this project | It queries `Win32_PhysicalMemory`; it is not a raw SPD reader |
| PowerShell `Get-CimInstance` | Reads `Win32_PhysicalMemory` directly | Shows the same Windows/SMBIOS layer |
| WMIC `memorychip` | Legacy WMI command-line view | Removed from Windows 11 version 24H2 and later |
| Module-compatible external SPD programmer | Reads, protects, writes, and verifies the SPD device | Must support the exact generation, device, connector, and protection mode |
| [Century Micro SPD PROGRAMMER 2](https://century-micro.co.jp/spdpgm2/spec.php) | Official example with read, write, verify, SWP, CWP, RPS, and binary backup | DDR4 only; the vendor does not guarantee third-party modules |

> [!WARNING]
> Seller-bundled editors and drivers are proprietary and unverified **[S]**. Use only the programmer vendor's authenticated distribution. Do not use mirrors, archives, cracks, or repacks.

## Read-only baseline

### 1. Record the Windows view **[A]**

Open PowerShell and run:

```powershell
Get-CimInstance Win32_PhysicalMemory |
    Select-Object DeviceLocator, Manufacturer, PartNumber, SerialNumber, Capacity |
    Format-Table -AutoSize
```

Save the result locally. Do not publish it. For structured output:

```powershell
Get-CimInstance Win32_PhysicalMemory |
    Select-Object DeviceLocator, Manufacturer, PartNumber, SerialNumber, Capacity |
    ConvertTo-Json -Depth 2
```

### 2. Check with HWIDChecker **[A]**

1. Run `HWIDChecker.exe` from the repository root.
2. Approve the Windows elevation prompt.
3. Open the `RAM MODULES` section.
4. Record the device locator, manufacturer, part number, capacity, and serial for each module.

The app requests administrator elevation and queries `Win32_PhysicalMemory`. Agreement with PowerShell confirms the same Windows/SMBIOS view.

### 3. Optional WMIC fallback **[A]**

On an older Windows installation that still includes WMIC:

```bat
wmic memorychip get DeviceLocator,Manufacturer,PartNumber,SerialNumber,Capacity
```

Microsoft removed WMIC as a Feature on Demand from Windows 11 version 24H2 and later. Use PowerShell on current systems.

### 4. Map rows to physical modules **[S]**

> [!WARNING]
> This slot-mapping procedure has not been physically tested by this project. Power down and disconnect power before moving modules.

If slot labels are unclear, record the rows, shut down, remove one module, and compare the rows after the next boot. Repeat only as needed. Never remove or insert a desktop DIMM while power is connected.

## External-programmer procedure

> [!WARNING]
> Every numbered step in this section is **[S]**. This procedure combines primary device documentation with an untested external workflow. Stop if the programmer's official instructions differ.

### 1. Isolate one module **[S]**

Shut down, disconnect external power, discharge residual power as the system manual directs, and use ESD protection. Remove only the target module. Keep a known-good module untouched.

### 2. Confirm exact support **[S]**

Before connecting the module, confirm:

- DDR4 or DDR5
- DIMM or SO-DIMM connector and orientation
- Exact SPD EEPROM or hub model
- Required supply and I/O voltages
- Full-dump size: 512 bytes for DDR4 or 1024 bytes for DDR5
- Supported read, verify, and write-protection operations
- DDR5 offline-tester support if block 8 is protected

Use the supplied socket or documented adapter. If the programmer uses USB, use a data-capable cable. Do not connect the module if the manual is ambiguous.

### 3. Read twice and back up **[S]**

1. Read the complete SPD address space.
2. Save it as `module-a-original-read-1.bin`.
3. Disconnect and reconnect the module as the programmer requires.
4. Read it again as `module-a-original-read-2.bin`.
5. Compare both files and their hashes.

```powershell
Get-FileHash .\module-a-original-read-1.bin -Algorithm SHA256
Get-FileHash .\module-a-original-read-2.bin -Algorithm SHA256
```

The lengths and hashes must match. Keep one copy offline. If they differ, stop.

### 4. Decode the untouched image **[S]**

Confirm that the decoded generation, capacity, organization, manufacturer, part number, and serial are plausible for the physical module. Verify the existing CRC fields before editing. A decoder that identifies the wrong generation or omits blocks is not safe to use.

### 5. Record protection before clearing it **[S]**

For DDR4, read the protection status for all four 128-byte blocks. Only block 2 contains the serial. If block 2 is already writable, do not clear all protection. If the exact device requires `VHV` to clear protection, use only a programmer that documents that operation.

For DDR5, record `MR12` and `MR13`. Only `MR13` bit 0 controls block 8. If it is set, use documented offline-tester mode. Do not change unrelated protection bits.

### 6. Change only the serial **[S]**

Preserve all other fields, including:

- Capacity and organization
- Timings and voltages
- Module and DRAM manufacturer IDs
- Manufacturing date, part number, and revision
- DDR4 extended-function data
- DDR5 PMIC, hub, and temperature-sensor data
- Overclocking profiles

The serial format is manufacturer-defined. For a four-byte hexadecimal display, fabricated examples are `7C31A942` and `7C31A943`. Use a different value for each physical module. These examples are invented and are not copied from a device.

Do not impersonate another product. If the editor cannot isolate bytes 325-328 on DDR4 or 517-520 on DDR5, stop.

### 7. Keep CRC bytes unchanged **[S]**

A serial-only edit is outside the defined CRC coverage on both generations:

- DDR4: do not change bytes 126-127 or 254-255.
- DDR5: do not change bytes 510-511.

If the tool insists on recalculating those fields after only a serial edit, stop and inspect its diff before writing.

### 8. Write once and read back **[S]**

Write the edited image once. Immediately read the full device into a new file. Use the programmer's verify function and compare the original, intended image, and readback.

For the narrow procedure in this guide, the only content differences should be:

- DDR4 bytes 325-328, or
- DDR5 bytes 517-520.

Any other changed byte means the write was broader than intended. Restore the original image before installing the module.

### 9. Restore the original protection state **[S]**

Restore each reversible protection bit to its recorded state. Read the protection map again. Do not introduce a new permanent lock or protect an area that was originally writable.

### 10. Boot with a recovery path **[S]**

Install the edited module only after a full readback matches the intended image. Keep the programmer and original backup available. Use default firmware memory settings for the first boot.

If the system does not complete POST, power it off. Do not keep retrying with aggressive memory settings.

## Verify the result

After a successful boot:

1. Run the PowerShell baseline command again. **[A]**
2. Run HWIDChecker and inspect `RAM MODULES`. **[A]**
3. Confirm that the intended row has the new serial. **[A]**
4. Confirm that manufacturer, part number, capacity, and locator are unchanged. **[A]**
5. Compare the external programmer's full readback with the intended image. **[S]**

> [!WARNING]
> The raw readback comparison in step 5 is part of the untested hardware procedure **[S]**. It is the only check here that verifies the SPD device rather than SMBIOS.

If the external readback is correct but Windows still shows the old value, do not rewrite the module repeatedly. Firmware controls the Type 17 string presented to Windows and may cache, normalize, omit, or replace the raw SPD value. **[A]**

## Recovery

> [!WARNING]
> This recovery procedure is **[S]** and untested. It requires a programmer that can access the target module without booting it.

If the module prevents POST or reports corrupted data:

1. Power off and remove the edited module.
2. Boot with a known-good module if the system supports that configuration.
3. Connect the failed module to the external programmer.
4. Read and save the failed state for diagnosis.
5. Write the untouched original full binary image.
6. Verify the entire device against that backup.
7. Restore only the original reversible protection state.
8. Test the restored module at default firmware memory settings.

If the programmer cannot identify the SPD device, clear the required reversible protection, or verify the restored image, stop. Use the module vendor's support or a qualified memory-repair service. Do not copy an SPD image from a merely similar module. Capacity, ranks, PCB layout, DRAM parts, revision, and support-device configuration can differ.

## Troubleshooting

| Symptom | Supported explanation | Safe response |
|---|---|---|
| PowerShell and HWIDChecker show the same value | Both read `Win32_PhysicalMemory` | Treat them as one SMBIOS observation layer. **[A]** |
| SPD reads but will not write in-system | Device protection or a platform write-disable can block writes | Read protection status and use the exact device and platform documentation. **[A]** |
| DDR4 block 2 is protected | EE1004 reversible protection covers bytes 256-383 | Use only the documented clear operation and record all four block states first. **[A]** |
| DDR5 `MR13` bit 0 is set | Protection block 8 contains bytes 512-575 | Use a programmer with documented SPD5118 offline-tester support. **[A]** |
| A serial-only edit changes a CRC | The editor is making a broader change than required | Cancel the write and inspect the binary diff. **[A]** |
| Windows stays unchanged after a verified raw write | Firmware controls SMBIOS Type 17 output | Keep the verified dump and stop repeated writes. **[A]** |
| Two untouched reads differ | Connection, voltage, or compatibility may be wrong | Stop before writing. **[S]** |
| RGB behavior changes | Non-SPD state may have been changed | Power off and restore the original image. **[S]** |
| The system no longer reaches POST | Configuration or support-device data may be damaged | Remove the module and follow [Recovery](#recovery). **[S]** |

> [!WARNING]
> The **[S]** troubleshooting responses above are untested stop-and-recover guidance. They do not prove that a damaged module is recoverable.

## Sources

- **[A] Windows exposure:** [Microsoft Win32_PhysicalMemory class](https://learn.microsoft.com/en-us/windows/win32/cimwin32prov/win32-physicalmemory)
- **[A] Current SMBIOS Type 17 fields:** [DMTF SMBIOS Specification DSP0134 3.10.0](https://www.dmtf.org/sites/default/files/standards/documents/DSP0134_3.10.0.pdf)
- **[A] DDR4 field layout and CRC coverage:** [JEDEC Annex L, Serial Presence Detect for DDR4 SDRAM Modules](https://www.jedec.org/sites/default/files/docs/4_01_02_AnnexL-3R25.pdf)
- **[A] Current DDR5 field layout and CRC coverage:** JEDEC JESD400-5D.01, available through [JEDEC Standards and Documents](https://www.jedec.org/standards-documents)
- **[A] DDR4 EE1004 and protection commands:** [Microchip AT34C04 datasheet](https://ww1.microchip.com/downloads/aemDocuments/documents/MPD/ProductDocuments/DataSheets/AT34C04_I2C-Compatible_4-Kbit_Serial_EEPROM_with_Reversible_Software_Write_Protection_20006416A.pdf)
- **[A] DDR5 SPD hub and protection blocks:** [Renesas SPD5118 product page](https://www.renesas.com/en/products/spd5118), including the SPD5108/SPD5118 datasheet R10DS0299EU0111, and [Montage M88SPD5118 product brief](https://www.montage-tech.com/uploads/files/202602/PB0034_M88SPD5118_ProductBrief_20251021.pdf)
- **[A] DDR5 PMIC and SPD hub roles:** [Micron DDR5 client module features](https://www.micron.com/content/dam/micron/global/public/products/white-paper/ddr5-key-module-features-wp-client.pdf)
- **[A] Intel platform write-disable example:** [Intel Core Ultra 200S/200HX Host Configuration register](https://edc.intel.com/content/www/us/en/design/publications/core-ultra-p200s-series-processors-soc-i-o-registers/001/host-configuration-hcfg-offset-40/)
- **[A] ESD-safe module handling:** [Kingston desktop DIMM installation](https://www.kingston.com/en/support/technical/how-to-install-memory-desktop-pc)
- **[A] WMIC removal status:** [Microsoft deprecated Windows client features](https://learn.microsoft.com/en-us/windows/whats-new/deprecated-features)
- **[A] HWIDChecker implementation:** [`RamInfo.cs`](../../app/src/Hardware/RamInfo.cs) and [`app.manifest`](../../app/src/app.manifest)
- **[A] DDR4 programmer capability example:** [Century Micro SPD PROGRAMMER 2 specifications](https://century-micro.co.jp/spdpgm2/spec.php)
