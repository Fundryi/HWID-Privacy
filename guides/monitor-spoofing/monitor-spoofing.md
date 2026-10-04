# Monitor and EDID Privacy Guide

> [!NOTE]
> **TL;DR:** Changes the EDID identity (manufacturer, model, serial) a monitor reports to the system, through a software override or an inline emulator. Direct EEPROM rewrite changes the display hardware itself.
> Who reads it: Windows (`WmiMonitorID`, registry EDID), the GPU driver, and any fingerprinting stack that reads display identity.
> **Status:** the EEPROM rewrite report is first-hand from 2013 Blur Busters / Toni Wilen **[C]**. The override and emulator methods are documented against vendor and Microsoft sources **[A]**.
> **Risk:** a bad EDID can remove the picture, hide valid resolutions, disable audio, HDR, VRR, or HDCP, and leave you without a usable recovery screen. Keep an untouched backup and a second display or direct-cable recovery path before changing anything.

Evidence grades appear inline. See [How to read these guides](../getting-started/getting-started.md#how-to-read-these-guides). This guide uses the same evidence model as the [fTPM identity reset guide](../resets/ftpm-reset-tutorial.md).

## Table of Contents

- [Overview](#overview)
- [What this changes](#what-this-changes)
- [How monitor identity is stored](#how-monitor-identity-is-stored)
- [Requirements](#requirements)
- [Tools](#tools)
- [Prepare a safe edited EDID](#prepare-a-safe-edited-edid)
- [Option 1: Windows software override](#option-1-windows-software-override)
- [Option 2: Programmable HDMI EDID emulator](#option-2-programmable-hdmi-edid-emulator)
- [Option 3: Other inline emulators](#option-3-other-inline-emulators)
- [Option 4: Direct monitor EEPROM modification](#option-4-direct-monitor-eeprom-modification)
- [Verify with HWIDChecker](#verify-with-hwidchecker)
- [Troubleshooting](#troubleshooting)
- [Sources](#sources)

## Overview

Extended Display Identification Data, or EDID, is the descriptor a display presents to a video source. It contains identity fields and operating capabilities such as supported timings, physical size, color data, audio information, HDR metadata, and variable-refresh information. Windows uses the descriptor to identify the display and choose valid modes. **[A]**

Monitor privacy work should change only the identity fields that need to change. Preserve every timing and capability block unless you intentionally want different behavior. Microsoft warns that invalid EDID data can make Windows choose incorrect display modes. **[A]**

There are three practical levels:

1. A **Windows EDID override** changes what one Windows installation supplies to its display stack. It does not rewrite the monitor. **[A]**
2. A **programmable inline emulator** presents its stored EDID to sources connected through it. This is reversible and does not open the monitor. **[A]**
3. A **direct EEPROM rewrite** changes the display hardware. It is model-specific, may require board work, and is not validated by this project. **[C]**

## What this changes

An EDID change can alter the monitor identity and capability data visible through the display connection. Windows exposes identifying data through `WmiMonitorID`, and the current HWIDChecker also reads the raw base-block numeric serial from the registry. **[A]**

It does **not** change:

- A serial printed on the case or stored elsewhere in monitor firmware.
- GPU, motherboard, storage, network, or operating-system identifiers.
- HDCP keys or certificates.
- The identity of an inline emulator itself on USB, Ethernet, or another management interface.
- Every possible display identifier. Modern displays can also carry product information in DisplayID data blocks, and specialized displays can expose additional vendor-defined data. **[A]**

The scope depends on the method:

| Method | Where the change is visible | Survives another PC or OS | Main limitation |
|---|---|---:|---|
| Windows override | That Windows display stack | No | Driver and connection-path behavior can differ |
| Inline HDMI emulator | Sources connected through the emulator | Yes | Device capacity, bandwidth, and protocol support |
| Direct EEPROM rewrite | The modified monitor input or board path | Yes | High hardware and recovery risk |

## How monitor identity is stored

### Base EDID block

EDID 1.x uses 128-byte blocks. The first block is the base block. Byte `0x7E` states how many 128-byte extension blocks follow, and byte `0x7F` is the base-block checksum. Each block has its own checksum. The unsigned sum of all 128 bytes in a block must equal zero modulo 256. **[A]**

The important base-block identity fields are: **[A]**

| Offset | Length | Field | Notes |
|---:|---:|---|---|
| `0x08` | 2 bytes | Manufacturer ID | Encoded three-letter PNP ID, not three ASCII bytes |
| `0x0A` | 2 bytes | Product code | Vendor-assigned, little-endian |
| `0x0C` | 4 bytes | Numeric serial | Optional 32-bit little-endian value |
| `0x10` | 1 byte | Week or model-year flag | `0x00` means unspecified, `0x01` to `0x36` mean weeks 1 to 54, and `0xFF` makes byte `0x11` a model year |
| `0x11` | 1 byte | Manufacture or model year | Stored as the year minus 1990 |
| `0x36`, `0x48`, `0x5A`, `0x6C` | 18 bytes each | Four timing or display-descriptor slots | A display descriptor has `00 00 00` in its first three bytes and its tag in byte 3 |

> [!NOTE]
> The 32-bit serial at `0x0C` and a display descriptor tagged `0xFF` are separate fields. The `0xFF` descriptor can hold up to 13 ASCII characters. Some displays use one, both, or neither. Changing only one can leave the other unchanged.

If a parser reports a DisplayID product-identification block, inspect it too. DisplayID is modular and can carry its own product information. Windows 11 can prefer qualifying DisplayID 2.0 data for many display properties, although legacy EDID still supplies PNP identification. Keep duplicated identity fields internally consistent. **[A]**

### Where Windows exposes EDID

For a normal enumerated monitor, the raw descriptor is commonly available as the binary `EDID` value below this per-device path:

```text
HKLM\SYSTEM\CurrentControlSet\Enum\DISPLAY\<monitor-id>\<instance>\Device Parameters\EDID
```

Windows can also store per-block overrides under an `EDID_OVERRIDE` key. Microsoft documents that these registry overrides take precedence over the corresponding EEPROM blocks. **[A]**

The `root\wmi:WmiMonitorID` class exposes the active instance, manufacturer name, product code, serial string, friendly name, and week/year of manufacture. **[A]**

### DDC, DDC/CI, HDMI, and DisplayPort

EDID reads and monitor controls are related but not interchangeable:

- Windows reads monitor EDID through the display path using DDC. Microsoft describes DDC as an I2C-based channel used to retrieve the monitor descriptor. **[A]**
- DDC/CI uses monitor-control commands such as brightness, contrast, input selection, and power state. Working DDC/CI controls do not prove that the EDID EEPROM is writable. **[A]**
- DisplayPort carries EDID access through its AUX channel using I2C-over-AUX. An HDMI-only emulator does not automatically intercept a native DisplayPort EDID path. **[A]**
- EEPROM write protection is chip-specific. For the Microchip/Atmel `AT24C02C`, asserting the hardware write-protect pin protects the full array. Do not assume the same pinout or behavior for another chip. **[A]**

## Requirements

Before changing anything, have all of the following:

- The monitor connected directly to the GPU through the input you plan to use.
- An untouched EDID binary exported from that exact connection path.
- A SHA-256 hash of the original file and a second backup stored elsewhere.
- A second monitor, another input, or a known recovery method.
- The monitor's native resolution, maximum refresh rate, HDR state, VRR range, audio formats, and color depth recorded before the change.
- An editor that understands every extension block in the file.
- For an inline device, confirmed capacity for the complete EDID and confirmed support for the required HDMI or DisplayPort link features.

> [!CAUTION]
> Do not start from a generic preset if the goal is to preserve the monitor's capabilities. Start from a dump of your own display, edit the minimum identity fields, and retain every block the target device can store.

## Tools

- **HWIDChecker.exe** from the repository root. Use it for the before-and-after identity comparison. The source of its monitor checks is [`monitor.rs`](../../app/rust/src/hw/monitor.rs). **[A]**
- **[MonitorInfoView](https://www.nirsoft.net/utils/monitor_info_view.html)**. It reads EDID records stored by Windows and can export an EDID to a binary file. **[A]**
- **[AW EDID Editor](https://www.analogway.com/products/aw-edid-editor)**. It creates and edits standard binary or text EDID files and supports EDID 1.3/1.4, CTA-861-G, and DisplayID 1.3 according to its vendor. **[A]**
- **[Custom Resolution Utility (CRU)](https://www.monitortests.com/forum/Thread-Custom-Resolution-Utility-CRU)**. It creates Windows software overrides. It does not rewrite display hardware. **[A]**
- A vendor-supported utility for the exact programmable emulator you own. Download it from the vendor page, not a repackaged archive.

> [!NOTE]
> CRU is useful for a Windows override and for inspecting display data. Exporting a file from CRU does not by itself program a monitor or inline emulator.

## Prepare a safe edited EDID

This preparation workflow is independent of the deployment option. **[A]**

### 1. Capture a baseline

1. Connect the display directly, without a dock, KVM, receiver, or emulator.
2. Run `HWIDChecker.exe` and save or screenshot the **MONITOR INFORMATION** section.
3. Export the active monitor's complete EDID with MonitorInfoView.
4. Record the file size. It should be a whole number of 128-byte blocks for an EDID 1.x file.
5. Hash the file:

   ```powershell
   Get-FileHash .\monitor-original.bin -Algorithm SHA256
   ```

6. Make two read-only copies. Never edit the only original.

### 2. Check the complete block count

Run this against the exported file:

```powershell
$edid = [IO.File]::ReadAllBytes((Resolve-Path .\monitor-original.bin))
[PSCustomObject]@{
    LengthBytes      = $edid.Length
    ExtensionCount   = $edid[0x7E]
    ExpectedLength   = 128 * (1 + $edid[0x7E])
}
```

If `LengthBytes` and `ExpectedLength` differ, stop. The export may be truncated, padded, or in a different format. Do not load a partial file into an emulator.

> [!WARNING]
> The Dr HDMI 4K vendor specification lists support for 256-byte extended EDID. A three-block 384-byte monitor EDID does not fit that documented limit. Do not truncate it to make it fit. **[A]**

### 3. Edit only identity fields

Open a copy in an EDID-aware editor. Preserve all timing and extension data. Change only the fields you have deliberately selected:

- Manufacturer PNP ID, only if you have a reason to change the reported vendor.
- Product code.
- Base-block 32-bit numeric serial.
- `0xFF` serial-text descriptor, if present.
- Week and year, only if you want those values to differ.
- Matching product-identification fields in DisplayID, if the file contains them.

A fabricated but structurally plausible example is `DEL` manufacturer ID, product code `6A37`, numeric serial `0x59C31A72`, and serial text `CN0X8P4R7N2Q`. Do not copy this exact example. Generate your own values and keep the three-letter manufacturer code valid.

> [!TIP]
> Leaving the real manufacturer and model capabilities intact while changing device-specific serial data produces fewer compatibility surprises than replacing the whole descriptor with an unrelated preset.

### 4. Validate every block

Before deployment:

1. Confirm the file length still matches `128 * (1 + extension count)`.
2. Confirm every 128-byte block sums to zero modulo 256.
3. Reopen the saved file in a second EDID parser.
4. Compare all timings and extension blocks with the original.
5. Confirm only the intended identity bytes and their block checksums changed.

Do not proceed if the parser reports a malformed block, missing extension, or unsupported data structure.

## Option 1: Windows software override

This is the lowest-risk way to test a changed identity on one Windows installation. It is not a hardware rewrite. Microsoft documents per-block EDID overrides through a monitor INF, and CRU provides a practical registry-based override workflow. **[A]**

CRU requires Windows Vista or later and a supported graphics driver. Its author states that the Microsoft Basic Display Adapter does not support EDID overrides. **[A]**

**Status:** documented override mechanism. **[A]**

1. Open CRU and select the active monitor. Confirm its current identity and native mode match your baseline.
2. Import the complete edited EDID. Use **Import complete EDID** when the intent is to import identity fields rather than only resolutions.
3. Click **OK** to save the override.
4. Run CRU's `restart.exe` or reboot. The upstream instructions state that this restarts the graphics driver and applies the registry override.
5. Run HWIDChecker and compare every monitor field.
6. Test native resolution, maximum refresh rate, HDR, VRR, audio, sleep/wake, and reconnect behavior.

Rollback:

1. Run CRU's `reset-all.exe`.
2. Reboot, or run `restart.exe`.
3. Verify that HWIDChecker again shows the baseline identity.

> [!NOTE]
> A software override can behave differently across GPU drivers and connection paths. CRU documents that some NVIDIA configurations ignore overrides while Display Stream Compression is active. **[A]**

## Option 2: Programmable HDMI EDID emulator

A programmable inline emulator is the preferred hardware-level option when it fully supports the original EDID size and the required signal features. It is reversible and leaves the monitor unopened. **[A]**

### Dr HDMI 4K

The official Dr HDMI 4K page documents 18 Gbps HDMI 2.0b operation, 40 banks, seven user-programmable banks, one sink-copy bank, 256-byte EDID support, and HDCP/CEC/lip-sync pass-through. **[A]**

1. Confirm the original EDID is no longer than 256 bytes and that 18 Gbps is sufficient for the target mode.
2. Connect the display to the device output and the GPU to its input. Connect USB power if required.
3. Use **Copy Sink** to capture the monitor EDID into the reserved sink-copy bank.
4. Save that copy through the official PC utility and compare it byte-for-byte with the direct baseline.
5. Edit the saved copy using the preparation workflow above.
6. Load the edited file into a user-programmable bank.
7. Select that bank. The vendor documents that the device issues a hot-plug event after an EDID selection so the source re-reads it.
8. Reboot the source if the old identity remains, then verify with HWIDChecker and complete the feature checks.

Rollback: select the sink-copy bank, load the untouched original into a custom bank, or remove the emulator and reconnect the monitor directly.

### Dr HDMI 8K

The official Dr HDMI 8K page documents HDMI 2.1 FRL6 at 48 Gbps, VRR, HDR formats, HDCP pass-through, ten user-programmable banks, sink copy, and EDID upload/download through its web interface. **[A]**

Firmware 1.4 added DisplayID 2.0 and 384-byte and 512-byte EDIDs in sink-copy, custom-bank, and automix modes. Update the device to current vendor firmware before relying on that support. Older firmware and the Dr HDMI 4K do not gain this capacity from the EDID file itself. **[A]**

Use the same clone, edit, validate, upload, select, and verify sequence. Confirm the installed firmware and accepted EDID size before uploading.

> [!CAUTION]
> Protocol support is a property of the whole inline device, not only of the EDID file. A preserved VRR, HDR, DSC, audio, or HDCP advertisement does not make an older emulator pass that feature. Match the device's documented bandwidth and protocol support to the original signal path.

## Option 3: Other inline emulators

### Generic HDMI EDID adapters

Some low-cost adapters present a fixed EDID. Others copy a sink EDID but cannot load an edited file. Product names and enclosures are not enough to tell which type you have. **[S]**

> [!WARNING]
> No generic adapter model or programming software is verified. Treat vendor claims as untested until you read back the exact EDID, compare all blocks, and complete the feature tests below. **[S]**

Before purchase or use, require all of these in the vendor documentation:

- User-programmable EDID, not only preset selection or sink cloning.
- A stated maximum EDID size that fits the original file.
- The exact input/output connector and protocol. HDMI-only is not native DisplayPort.
- Enough link bandwidth for the target resolution, refresh rate, color depth, and chroma.
- Explicit HDR, VRR, DSC, audio, CEC, and HDCP pass-through support where needed.
- A documented factory reset or original-EDID restore path.

### Dichen 5 programmable fuser

The external source describes a `DC240HZ5D-2` unit with a USB-C programming port, a CH-series USB serial interface, and a Windows flash utility. It says an exported EDID binary is loaded in the utility, written through the correct COM port, and selected for the target resolution. **[S]**

> [!WARNING]
> This Dichen workflow is untested. No authoritative manufacturer manual, supported EDID-size statement, checksum behavior, clean upstream utility, or recovery procedure is cited. Do not use a third-party archive on a primary Windows installation. **[S]**

If you independently obtain authoritative instructions for your exact revision, the safe sequence is still: direct baseline, full readback, edited copy, checksum validation, write to a recoverable slot, readback comparison, then HWIDChecker and signal-feature verification. Every Dichen-specific action remains **[S]** until physically tested and documented.

## Option 4: Direct monitor EEPROM modification

**Status:** first-hand 2013 report for the ASUS VG248QE, credited to Toni Wilen. **[C]** Untested by this project.

A 2013 Blur Busters report for the ASUS VG248QE, credited to Toni Wilen, identified a separate eight-pin `AT24C02C` behind the DVI connector, isolated its write-protect pin from the board, and then rewrote the DVI EDID through the display connection. It also reported a separate EEPROM for HDMI. This layout and result are specific to that monitor and board revision. They do not establish the layout of another monitor or its DisplayPort path. **[C]**

> [!WARNING]
> This knife/solder procedure is untested by this project. Opening a monitor can expose charged high-voltage sections even after unplugging it. Board damage, electric shock, fire, data loss, and a permanently unusable display are possible. Do not cut, desolder, or lift a pin based on a photograph from another model. Use a qualified repair technician. **[C]**

The verified component-level fact is narrower. The Microchip/Atmel `AT24C02C` datasheet assigns write protect to pin 7 in its listed eight-pin packages. Connecting that pin to `VCC` inhibits writes to the full array. Connecting it to ground permits normal writes. A floating pin is internally pulled down, but Microchip recommends driving it to a known state. This does not identify the chip in your monitor or validate an in-circuit modification. **[A]**

Minimum evidence required before any board work:

- A service manual or traced schematic for the exact monitor revision.
- A legible chip marking and the exact manufacturer's datasheet.
- Confirmation of which EEPROM belongs to the target input.
- Two matching full-chip dumps made with the monitor safely isolated.
- A programmer that supports the chip voltage and page-write rules.
- A tested plan to restore the original dump if the monitor no longer enumerates.
- Electronics safety experience appropriate for mains-powered display hardware.

Until all of those are available, use a software override or a programmable inline emulator instead.

## Verify with HWIDChecker

Verification is not just "the picture came back." Use the same connection path before and after. How to run and export: [Take before and after snapshots](../getting-started/getting-started.md#take-before-and-after-snapshots).

1. Run `HWIDChecker.exe` from the repository root.
2. Open the **MONITOR INFORMATION** section.
3. Compare these fields with the saved baseline:
   - **Manufacturer**
   - **Model**
   - **Serial Number**
   - **Product Code**
   - **Manufacturing Date**
   - **EDID Serial (numeric)**, when the base-block value is not a placeholder
4. Confirm every field you intended to change is different.
5. Confirm every field you intended to preserve is unchanged.
6. Disconnect and reconnect the display, reboot Windows, and check again.
7. For an inline emulator, connect the chain to a second computer and repeat the check. This distinguishes a hardware-path result from a local Windows override.

HWIDChecker first queries `root\wmi:WmiMonitorID`. **Manufacturer**, **Model**, **Serial Number**, **Product Code**, and **Manufacturing Date** come from that WMI class. It separately reads bytes `0x0C` to `0x0F` from a registry EDID and prints the unsigned value in decimal as **EDID Serial (numeric)**. **[A]**

The Rust numeric-serial lookup uses the exact WMI device instance to read its registry EDID. It never substitutes another monitor with the same manufacturer. Registry-derived identity and extension data retain their source labels; read or checksum failures are recorded in diagnostics. Registry data is cached Windows state, so confirm the raw EDID when a fresh hardware read matters. **[A]**

If WMI fails or returns no monitors, HWIDChecker scans the registry instead. That fallback can include disconnected entries, marked **Presence: Not connected** when a successful SetupAPI snapshot confirms absence. It displays **Manufacturer**, the `0xFC` **Model**, the `0xFF` **Serial Number**, and the numeric serial when present, plus registry-labeled product code and available manufacturing/model-year fields. **[A]**

After identity verification, test the real display behavior:

- Native resolution and maximum refresh rate.
- HDR enablement and correct color depth.
- VRR or Adaptive-Sync across the expected range.
- Audio formats and channel count.
- HDCP-protected playback on content you are authorized to view.
- Sleep, wake, reboot, cable reconnect, and input switching.

## Troubleshooting

### No signal or wrong modes

- Bypass the emulator or select its sink-copy/original bank.
- For CRU, use its recovery mode or `reset-all.exe`, then restart the driver or reboot.
- Restore the untouched EDID.
- Check that extension blocks were not removed and that each checksum is valid.
- Use the second display to recover Windows settings.

### Identity did not change

- Confirm the edited bank is active.
- Trigger a hot-plug event, power-cycle the emulator, and reboot the source.
- Remove docks, KVMs, receivers, and adapters while diagnosing.
- Confirm you changed both the numeric serial and the `0xFF` text serial when both exist.
- Check for a DisplayID product-identification block that still carries old data.
- Make sure you are looking at the active WMI instance rather than a disconnected registry entry.

### HDR, VRR, audio, or HDCP disappeared

- Compare the original and presented EDID block-for-block.
- Confirm the inline device supports the link bandwidth and the missing protocol.
- Do not truncate a 384-byte or larger EDID to fit a 256-byte device.
- Restore the original capability blocks. Change only the required identity fields.
- Test direct-to-monitor. If the feature returns, the emulator or edited EDID is the cause.

### DisplayPort path still shows the original identity

An HDMI emulator cannot intercept a native DisplayPort AUX transaction. Use a DisplayPort-aware device, a Windows override, or a validated rewrite method for the monitor's DisplayPort EDID storage. **[A]**

### DDC/CI works but EDID writing fails

Brightness or input control through DDC/CI does not prove EEPROM write access. The EDID storage can be hardware- or software-protected even while reads and monitor-control commands work. Identify the exact EEPROM and protection design before drawing conclusions. **[A]**

### The file parses but HWIDChecker shows two different serials

HWIDChecker intentionally reports the WMI `SerialNumberID` and the 32-bit base-block serial as separate values when both are available. On the registry fallback path, **Serial Number** is read from the `0xFF` descriptor. Compare the raw EDID before assuming which field supplied the WMI value. If both EDID serial fields should change, edit them separately, recalculate the affected block checksum, and verify again.

## Sources

- [Microsoft: WmiMonitorID class](https://learn.microsoft.com/en-us/windows/win32/wmicoreprov/wmimonitorid)
- [Microsoft: Using an INF file to override EDIDs](https://learn.microsoft.com/en-us/windows-hardware/drivers/display/overriding-monitor-edids)
- [Microsoft: Monitor class function driver](https://learn.microsoft.com/en-us/windows-hardware/drivers/display/monitor-class-function-driver)
- [Microsoft: Display component guidelines](https://learn.microsoft.com/en-us/windows-hardware/design/component-guidelines/display/)
- [Microsoft: About monitor configuration and DDC/CI](https://learn.microsoft.com/en-us/windows/win32/monitor/about-monitor-configuration)
- [VESA: Free standards, including E-EDID Release A Revision 2, E-DDC, DisplayID, and DDC/CI](https://vesa.org/vesa-standards/)
- [VESA: DisplayPort AUX and I2C-over-AUX overview](https://www.vesa.org/wp-content/uploads/2011/01/ICCE-Presentation-on-VESA-DisplayPort.pdf)
- [Linux DRM EDID structure definitions](https://github.com/torvalds/linux/blob/master/include/drm/drm_edid.h)
- [Microchip: AT24C01C/AT24C02C datasheet](https://ww1.microchip.com/downloads/en/DeviceDoc/AT24C01C-AT24C02C-I2C-Compatible-Two-Wire-Serial-EEPROM-1Kbit-2Kbit-20006111A.pdf)
- [Analog Way: AW EDID Editor](https://www.analogway.com/products/aw-edid-editor)
- [ToastyX: Custom Resolution Utility](https://www.monitortests.com/forum/Thread-Custom-Resolution-Utility-CRU)
- [NirSoft: MonitorInfoView](https://www.nirsoft.net/utils/monitor_info_view.html)
- [HDFury: Dr HDMI 4K](https://www.hdfury.com/product/dr-hdmi-4k/)
- [HDFury: Dr HDMI 8K](https://www.hdfury.com/product/dr-hdmi-8k/)
- [HDFury: Dr HDMI 8K user manual](https://www.hdfury.com/docs/HDfuryDr8K.pdf)
- [HDFury: Dr HDMI 8K firmware 1.4 EDID-size support](https://hdfury.com/dr-hdmi-8k-fw-1-4-now-available/)
- [Blur Busters: ASUS VG248QE hardware EDID modification report](https://blurbusters.com/zero-motion-blur/hardware-mod/)
