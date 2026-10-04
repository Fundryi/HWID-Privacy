# MAC Address Spoofing Guide

> [!NOTE]
> **TL;DR:** Changes the MAC address your network presents. A Windows `NetworkAddress` override is software-only and reversible; writing the controller's EEPROM, flash, OTP, or eFuse is permanent.
> Who reads it: the local network, DHCP servers, any fingerprinting stack that enumerates adapters, and anti-cheats that log NIC identity.
> **Status:** the ConnectX-3 procedure was tested on real hardware **[C]**. The Intel, Realtek onboard, and ASIX procedures are community-tested or untested **[S]** unless marked.
> **Risk:** an eFuse or OTP write is one-time. A wrong write can break the adapter.
> System requirements: Windows 10/11 for the Windows methods; DOS boot for Intel onboard NICs.

A software MAC override and a hardware-programmed MAC are not the same thing. EEPROM and flash may be rewritable. eFuse or OTP storage is one-time programmable. Back up every readable value before writing, but do not assume that a backup can undo an eFuse write.

Evidence grades appear inline. See [How to read these guides](../getting-started/getting-started.md#how-to-read-these-guides).

## Quick Navigation

| Category | NIC | Speed | Difficulty | Method |
|---|---|---|---|---|
| **Reference** | [Current and permanent MAC addresses](#current-mac-permanent-mac-and-burned-in-storage) | N/A | Read before choosing a method | Microsoft NDIS documentation |
| **Windows** | [Software-only override](#windows-networkaddress-override-software-only) | Any supported speed | Easy and reversible, but driver support varies | `Set-NetAdapter` / `NetworkAddress` |
| **System** | [Intel NICs](#intel-nics) | 1 GbE | Hard - requires DOS boot, BIOS changes; may fail on some chipsets | EEUPDATE via DOS boot USB |
| **System/PCIe** | [Realtek NICs](#realtek-nics) | 1-2.5 GbE | Medium - tools are trial-and-error; depends on chipset | eFuse Programmer |
| **USB** | [USB NICs overview](#usb-nics) | 1-2.5 GbE | Depends on controller, board revision, and storage | Controller-matched vendor tool |
| **USB** | [Realtek USB NICs](#realtek-usb-nics-update) | 2.5 GbE | Easy - plug in, run tool, done | Realtek USB PG Tool |
| **USB** | [TP-Link UE300](#tp-link-ue300--rtl8153) | 1 GbE | Medium - controller is known, storage implementation still must be identified | Realtek USB PG Tool |
| **USB** | [ASIX AX88179](#asix-ax88179ab-now-too) | 1 GbE | Easy - widely available, simple tool | ASIXFlash / Captain Mac Tool |
| **PCIe** | [Mellanox ConnectX-3](#mellanox-connectx-3-cx311a--mcx311a-xcat) | 10 GbE | Easy spoof, harder sourcing - commands are simple but finding the right card takes research | WinMFT flint (firmware flash) |
| **Reference** | [Controller storage](#controller-storage-efuse-eeprom-or-flash) | N/A | Read before any hardware write | Vendor documentation |
| **Reference** | [Verification checklist](#verification-checklist) | N/A | Readback in several views | HWIDChecker / PowerShell / `getmac` |
| **Reference** | [Sources](#sources) | N/A | Primary sources and evidence provenance | Microsoft / IEEE / vendors |

## Current MAC, Permanent MAC, and Burned-In Storage

Windows NDIS exposes a **current** MAC address and a **permanent** MAC address as separate adapter attributes. A driver may replace the current address with a software-configured value while the device's non-volatile storage remains unchanged. [A]

- A `NetworkAddress` override changes the address presented by the Windows driver. It does not rewrite the NIC's EEPROM, flash, or eFuse. [A]
- A vendor programming utility can change the address loaded from device storage. Whether that write is reversible depends on the actual storage fitted or enabled on that board. [A]
- `Get-NetAdapter`, `getmac`, `ipconfig /all`, and ordinary WMI queries report the active Windows address. They do not prove that the burned-in value changed. [A]

Realtek and ASIX controller families support more than one storage design. A controller name alone does not identify the storage used on a finished adapter. See [Controller Storage: eFuse, EEPROM, or Flash](#controller-storage-efuse-eeprom-or-flash). [A]

## Windows `NetworkAddress` Override (Software Only)

Microsoft documents `NetworkAddress` as the registry value used by NDIS drivers that support software-configurable addressing. `Set-NetAdapter -MacAddress` sets the current address and saves it to the network-address property without dashes. Not every adapter or driver supports this. [A]

This is non-persistent at the **hardware** level, but it is not necessarily session-only. The Windows setting can survive reboot until it is reset or removed. [A]

> [!WARNING]
> This operation briefly restarts the adapter and can disconnect remote sessions. A duplicate or invalid MAC can also break network access. Record the adapter name, current address, and advanced-property values first.

**Status:** documented Windows mechanism. **[A]**

1. Open PowerShell as Administrator and identify the target physical adapter:

   ```powershell
   Get-NetAdapter -Physical | Format-Table Name, InterfaceDescription, MacAddress, Status
   ```

2. Record the current address and any existing `NetworkAddress` property:

   ```powershell
   Get-NetAdapterAdvancedProperty -Name "Ethernet" -RegistryKeyword "NetworkAddress" -AllProperties
   ```

   If no row is returned, the property may not exist yet or the driver may not expose it.

3. Set a fabricated address. This example keeps Realtek's `00:E0:4C` OUI and changes the device-specific bytes:

   ```powershell
   Set-NetAdapter -Name "Ethernet" -MacAddress "00-E0-4C-5A-71-2D"
   ```

4. Verify the active address with the commands in the [verification checklist](#verification-checklist).

5. To return an exposed advanced property to its factory default, first obtain its exact `DisplayName`, then reset that property:

   ```powershell
   Get-NetAdapterAdvancedProperty -Name "Ethernet" -AllProperties |
     Where-Object RegistryKeyword -eq "NetworkAddress" |
     Format-List Name, DisplayName, RegistryKeyword, RegistryValue

   Reset-NetAdapterAdvancedProperty -Name "Ethernet" -DisplayName "Network Address"
   ```

   The display name is driver-defined and can be localized. Use the value returned on your system instead of assuming that it is exactly `Network Address`.

> [!NOTE]
> IEEE addressing defines universal/local and individual/group bits. A locally administered unicast address sets the local bit and clears the group bit. This guide's software example instead preserves a real vendor OUI to follow the project's example-address rule. In either case, the address must be unicast and unique on the local network. [A]

## Intel NICs

**Status:** untested as a general procedure. **[S]** Intel release notes confirm EEUPDATE can program MAC data on specific controllers, but they also document controller-specific failures, locked fields, checksum errors, and version-dependent behavior. Do not infer support from the Intel brand alone.

### Prerequisites

- Download required tools:
  - [EEUPDATE Utility](intel/EEupdate_5.35.12.0.zip)

For Intel network cards, you can use the EEUPDATE utility through a DOS bootable USB.

### Intel Tool Caveats and Backup

- Run `EEUPDATE /LIST_NIC` before using `/NIC=1`. Adapter numbering can change when cards are added, removed, enabled, or disabled. [S]
- Save the original MAC and the complete output of `EEUPDATE /MAC_DUMP` before any write. Keep that record off the boot USB as well. [S]
- Intel documents that some multi-port adapters share one EEPROM/flash and that its NVM Update Tool updates only port 0, or the port selected by MAC. Treat port selection and shared-storage behavior as controller- and tool-specific. [A]
- Tool version matters. Intel release notes document an I210 MAC-programming checksum failure in EEUPDATE 5.39.56.8 and later that was fixed in a subsequent release. [A]
- Some Intel device-ID and MAC fields can be locked. If the utility reports a lock, unsupported controller, checksum failure, or NVM error, stop. Do not try random older binaries or force flags. [A]
- Treat a successful command as incomplete until you power-cycle and compare the device readback with Windows' current address. [S]

### Setup Steps

1. Create a bootable DOS USB:

   - Download Rufus (https://rufus.ie)
   - Insert your USB drive
   - Select "MS-DOS" as the boot selection
   - Create the bootable drive

2. Prepare files:
   - Copy EEUPDATE.exe to your bootable USB
   - Create changemac.bat with the following content:

```batch
@echo Off
echo Update your current mac?
pause
echo Current MAC
Eeupdate.exe /NIC=1 /MAC_DUMP
echo Updating MAC
Eeupdate.exe /NIC=1 /mac=REPLACEMEWITHMAC
echo Updated MAC
Eeupdate.exe /NIC=1 /MAC_DUMP
echo If the above did not work type the following manually:
echo EEUPDATE /NIC=1 /mac=REPLACEMEWITHMAC
echo EEUPDATE /NIC=1 /MAC_DUMP
echo Last command will display the current MAC(if it worked, should display new one)
pause
```

- Example MAC: `AA:BB:CC:DD:EE:11`
  - Do not use this MAC, it will brick your network...

3. BIOS setup:
   - Enter BIOS (usually F2 or Delete key during startup)
   - Disable Secure Boot
   - Enable CSM (Compatibility Support Module) mode
   - Save changes and restart

### Running the Script

1. Boot from USB:

   - Insert the USB drive
   - Boot into DOS (may require selecting boot device during startup)
   - At the DOS prompt (A:\> or similar)
   - Type the first few letters of "changemac" and press TAB
     - In DOS, TAB will auto-complete the filename
     - Press Enter to run the script
   - Follow the prompts

2. Manual commands (if the script fails):

   ```dos
   EEUPDATE /NIC=1 /mac=AABBCCDDEE11
   EEUPDATE /NIC=1 /MAC_DUMP
   ```

3. After completion:
   - Remove the USB drive
   - Restart the system
   - Boot back into Windows to verify the change
   - Revert your Secure Boot and CSM settings.

### Important Notes

- Replace `AABBCCDDEE11` with your desired MAC address
- Keep your original MAC address noted down
- The `/NIC=1` parameter targets the first network adapter
  - If you have multiple, make sure either to change both or disable the one you don't need/use.
  - `EEUPDATE /LIST_NIC` will list the NICs installed.
- Some systems may require specific versions of EEUPDATE
- Not all Intel NICs support MAC address modification
- Incorrect MAC address format can cause network issues

## Realtek NICs

**Status:** untested as a general procedure. **[S]** The utility supports several controller families and storage modes, but a matching family name does not prove that the supplied configuration file fits your exact silicon and board.

The configuration file must match the exact NIC. No cited Realtek document establishes a dry-run selection workflow. Treat `8168FEF.CFG` and the supplied batch file as model-specific unless the utility identifies them as compatible. [S]

Realtek documents both embedded OTP storage and external serial EEPROM support on controllers such as RTL8125BG. OTP is one-time programmable. EEPROM is normally rewritable. Confirm the utility's selected mode before running `WINPG64.BAT`. [A]

The sample utility output reports `EFuse Remain`. Treat that as a one-time-capacity warning, not proof that the change can be undone. Before programming, capture the controller PCI hardware ID, current `NODEID`, storage mode, full utility output, and any available dump. [S]

### Prerequisites

- Download required tools (trial and error):
  - [RealTekNicPgW2.7.5.0.zip](realtek/RealTecNicPgW2.7.5.0.zip)
  - [realtek_efuse_prog.zip](realtek/realtek_efuse_prog.zip)

For Realtek network adapters, you can modify the MAC address using the Realtek eFuse Programmer toolkit.

### Programming Steps

1. Modify the MAC address:

   - Open the `8168FEF.CFG` file
   - Edit the first line to set your desired MAC address:
     ```
     NODEID = 00 E0 4C 88 00 18
     ;ENDID = 00 E0 4C 68 FF FF
     ```

2. Run the programming script:

   - Execute `WINPG64.BAT`
   - A successful rewrite will show output similar to:

     ```
     ****************************************************************************
     *       EEPROM/EFUSE/FLASH Windows Programming Utility for                 *
     *    Realtek RTL8136/RTL8168/RTL8169/RTL8125 Family Ethernet Controller  *
     *   Version : 2.69.0.3                                                    *
     * Copyright (C) 2020 Realtek Semiconductor Corp.. All Rights Reserved.    *
     ****************************************************************************

     PG EFuse is Successful!!!
     NodeID = 00 E0 4C 88 00 18
     EFuse Remain 61 Bytes!!!
     ```

3. Verify the MAC address change:
   - Open PowerShell
   - Run `ipconfig /all`
   - Look for your network adapter's Physical Address
   - It should match your programmed MAC address

## USB NICs

### Realtek USB NICs (Update)

**Status:** first-hand report by the named contributor Exclusion, on a Belkin USB-C-to-Ethernet adapter built on RTL8153. Not repeated by this guide's maintainer. [C] The bundled steps use PG Tool 2.0.22; the detailed report used 2.0.26.0. **[S]**

- Realtek-based USB NICs (e.g., RTL8153/RTL8156 series) can also be permanently spoofed.
- Use the Realtek USB PG Tool package; primary folder to use:
  - "**LATEST_PUB_WIN_USB_PGTOOL_v2.0.22_V2**"
- Tool package:
  - [RealtekMAC USB.zip](./usb-realtek/RealtekMAC%20USB.zip)
    - Older folders inside are retained only for experimentation; the above folder is the recommended one.
- Tested hardware:
  - Recommended USB NIC:
    - [USB‑C 2.5GbE (Uniaccessories)](https://uniaccessories.com/products/usb-c-to-ethernet-adapter-2500mbps)
      - [Amazon DE Link](https://www.amazon.de/-/en/dp/B0C2H9HVH3)
    - Examples that **DON'T WORK** at the moment because of missing .CFG settings or custom EFUSE:
      - [UGREEN Product](https://eu.ugreen.com/de/products/ugreen-usb-c-auf-ethernet-adapter)
      - [Amazon DE](https://www.amazon.de/dp/B0DNSTHRGQ/)

> [!CAUTION]
> Realtek documents that RTL8153B and RTL8156B controllers can use embedded OTP in place of an external EEPROM. The finished adapter decides which storage is present or enabled. If the tool is in `EFUSE` or `OTP` mode, assume that every write is permanent and consumes finite capacity. A dump is still mandatory, but it is not an undo button. [A]
> Match USB identifiers, record extra firmware and storage fields, limit retries, and perform device-level persistence checks. These controls were not part of Exclusion's report. [S]
> No cited vendor source establishes that configuration files or controller tables are interchangeable between PG Tool 2.0.22 and 2.0.26.0. Treat a version that cannot identify the exact controller as unsupported. [S]

- Quick programming steps (Windows):
  - Open the USB PG Tool from "LATEST_PUB_WIN_USB_PGTOOL_v2.0.22_V2".
  - Select your device and make sure mode is set to EFUSE.
  - Click "DUMP" to read current settings and confirm the tool returns "PASS".
![DUMP/Read section](./images/Realtek%20USB1.png)
  - Set "CURRENT MAC" to your desired value (preserve vendor OUI if possible).
  - Click "PROGRAM" to flash; success should show "PASS".
![DUMP/Read section](./images/Realtek%20USB2.png)
  - Done
- Serial number note:
  - The tool allows changing the USB "Serial Number". Avoid changing it in most scenarios:
    - Many Realtek USB NICs share common serial prefixes (e.g., "4013"), so altering it can make your unit uniquely stand out.
  - Do not modify other advanced settings unless you know exactly what they do.

#### Realtek USB PG Tool Details

The detailed first-hand workflow:

1. Unplug the Ethernet cable and leave only the target USB NIC connected. [C]
2. Use `SEARCH` to enumerate supported devices and select the target adapter. [C] Match the controller, USB VID/PID, and current MAC before continuing. [S]
3. Use `DUMP` first and record the current MAC plus any `MINIMUM` and `MAXIMUM` values. [C] Also record USB identifiers, firmware values, selected storage mode, tool version, and a redacted screenshot. [S]
4. Select `Program NODEID only`. Do not program the full configuration merely to change the MAC. [C]
5. Change only the device-specific bytes, preserve the vendor OUI, and keep the address inside the range reported for that unit. [C]
6. Select `Program` and save the result. [C] Do not retry a failed eFuse write repeatedly; record any remaining-byte counter. [S]
7. Reboot or unplug and reconnect the adapter, then verify the active address. [C] Use the [verification checklist](#verification-checklist) before making a device-storage claim. [S]

### TP-Link UE300 / RTL8153

**Status:** one successful report, revision and storage mode unconfirmed. **[S]**

TP-Link's UE300 product page and V1, V3, and V4 datasheets identify the controller as Realtek RTL8153. Its support page lists hardware versions from V1 through V5.60. These pages do not identify the active MAC storage for each revision, so confirm the version printed on the unit before using a low-level tool. [A]

> [!WARNING]
> A successful UE300 programming result was reported, but the available readback detail does not establish which UE300 hardware revision or storage mode was used. The steps below are a cautious application of the Realtek workflow, not a verified UE300 recipe. [S]

1. Record the UE300 model and hardware version from its label.
2. In Device Manager, record the USB hardware IDs and confirm that Windows identifies a Realtek-based adapter.
3. Open the Realtek USB PG Tool and use `SEARCH`. Stop if it does not identify the controller cleanly.
4. Use `DUMP` before any write. Save the original MAC, VID/PID, serial, storage mode, configuration range, firmware fields, and the complete tool output.
5. If the tool explicitly supports the unit, select `Program NODEID only` and change only the device-specific MAC bytes.
6. If `EFUSE` or `OTP` is selected, treat the write as irreversible. Do not experiment with serial, VID/PID, LED, or firmware fields.
7. Remove power from the adapter, reconnect it, and verify in HWIDChecker, `Get-NetAdapter`, and `getmac /v`.

Do not interpret a Windows-only address change as proof that the UE300's device storage changed. Clear any `NetworkAddress` override before testing persistence.

### ASIX AX88179(A/B now too!)

**Status:** short workflow from community use; the detailed Captain workflow is a first-hand third-party report. [C] Identify the exact controller and storage before using either. [S]

- Overview:
  - Permanent MAC changes are possible using the ASIX programming utility.
  - Keep the vendor OUI (first 6 hex digits) and change only the last 6.
- Downloads:
  - [ASIXFlash-master.zip](./usb-ax88179/ASIXFlash-master.zip)
    - Upstream reference: [ASIXFlash Repository](https://github.com/jglim/ASIXFlash)
  - [Captain Mac Tool.zip](./usb-ax88179/Captain%20Mac%20Tool.zip)
    - Password used: `captaindma`
      - Not added by me; it will also open their website...

- Quick steps:
  1. Extract the tool, run as Administrator.
  2. Backup current config/EEPROM if the tool provides an option.
  3. Program a new MAC that preserves the original OUI.
  4. Unplug/replug the adapter.
  5. Done
- Notes:
  - AX88179 "A/B" revisions can only be flashed with the Captain Mac Tool.
  - If programming fails or reverts, the unit/firmware may be locked or unsupported.

#### AX88179 Storage and Captain Tool Detail

ASIX documents different non-volatile storage by revision. The original AX88179 can use an external 93C56/93C66 EEPROM or embedded eFuse. AX88179A supports embedded eFuse plus external SPI flash. AX88179B uses embedded eFuse for device data. ASIX also documents its own Windows/Linux programming tools for these parts. [A]

ASIX says AX88179A and AX88179B controllers ship with a unique MAC address. For original AX88179-based designs, its FAQ instead directs the manufacturer to assign a unique MAC in EEPROM or eFuse. [A]

The existing statement that A/B revisions "can only be flashed with the Captain Mac Tool" describes the tooling bundled with this guide. It is not a vendor-wide limitation. Prefer the ASIX tool for the exact controller revision when ASIX makes it available to you. [A]

The external report describes this Captain workflow:

1. Extract the existing Captain package without running the executable yet.
2. Inspect `WIN X64 Drivers\179_178ATest.inf`. In Device Manager, update the target `ASIX USB to Gigabit Ethernet Family Adapter` manually and select the test driver as the external report describes. [C] Continue only if the INF hardware IDs match the adapter. [S]
3. Confirm that Device Manager shows the intended test-driver device with no error before opening the programming tool. [C]
4. Run `Captain Mac Tool.exe` as Administrator and select `Based On Mac Address`. [C]
5. Record the current value. Preserve the first three vendor bytes and change only the final three bytes. [C]
6. Select `Program`, wait for completion, then unplug and reconnect the adapter. [C]
7. Unplug and reconnect the adapter, then verify the active address with `ipconfig /all`. [C] Use the [cold-power persistence check](#5-cold-power-persistence-check) for stronger evidence. If the address reverts, do not repeat writes blindly. [S]

> [!CAUTION]
> eFuse cannot be erased. On an AX88179, AX88179A, or AX88179B board that uses eFuse, a successful backup cannot restore the consumed bits. On an EEPROM- or SPI-flash-based board, restoration still requires a controller-matched image and tool. [A]
> Match the INF to the adapter's hardware ID and perform a cold-power persistence check. Do not disable Windows driver-signing protections to force the driver to load. [S]

## Mellanox ConnectX-3 (CX311A / MCX311A-XCAT)

**Status:** verified working procedure, tested on Windows 10 with a real CX311A single-port SFP+ card. **[C]**

> [!IMPORTANT]
> The MAC change here is **device-level and persistent** (burned into NIC firmware), not an OS-level override.
> Unlike Intel X550 which has one-time-lock behavior, **ConnectX-3 supports repeated MAC changes**.

### Hardware Details

| Detail | Value |
|---|---|
| Card Model | CX311A / MCX311A-XCAT |
| Ports | Single SFP+ |
| PCIe | x4 |
| PSID | MT_1170110023 |
| Firmware | 2.33.5220 |
| Image Type | FS2 |
| Device ID | 4099 |

- RJ45 connectivity was provided through an SFP+ to RJ45 transceiver module.
  - Tested transceiver: [Tecowin SFP-10G-T-ME (Mellanox compatible)](https://www.tecowin.de/produkt/transceiver/sfp-10g-t/?attribute_pa_kompatibilitaet=mellanox&attribute_pa_modell=sfp-10g-t-me)
  - Any 10GBase-T SFP+ module with Mellanox compatibility should work.
- Internet and 10 Gbps link were already working before any flashing.
- Link stayed working at 10 Gbps after the MAC change.
- **Sourcing**: search for `Mellanox ConnectX-3 CX311A MCX311A-XCAT PCIe x4 SFP+` on eBay or AliExpress. These cards are widely available used.

### Prerequisites

- **OS**: Windows 10 / Windows 11 (the tested procedure below was on Windows 10, but the WinOF 5.50 driver and WinMFT 4.13 package are also known to work on Windows 11)
- **Driver**: WinOF 5.50.53000 - **not** WinOF-2 (ConnectX-3 is on the older WinOF branch)
- **Firmware tools**: WinMFT 4.13.3

Download both installers:
- [MLNX_VPI_WinOF-5_50_53000_All_Win2019_x64.zip](mellanox-connectx/MLNX_VPI_WinOF-5_50_53000_All_Win2019_x64.zip) - WinOF driver package
- [WinMFT_x64_4_13_3_6.zip](mellanox-connectx/WinMFT_x64_4_13_3_6.zip) - firmware tools (flint, mst, etc.)

> [!NOTE]
> The WinOF installer filename says "Win2019" - this refers to the build target (Windows Server 2019), but the driver installs and works correctly on Windows 10 and Windows 11 desktop as well.

> [!IMPORTANT]
> ConnectX-3 / ConnectX-3 EN requires **WinOF** (not WinOF-2). WinOF-2 is for ConnectX-4 and newer. Using the wrong driver package will fail silently or cause detection issues.

### Installation

1. Install the WinOF driver package first:
   - Run `MLNX_VPI_WinOF-5_50_53000_All_Win2019_x64.exe`
   - Follow the installer prompts, reboot if asked

2. Install WinMFT:
   - Run `WinMFT_x64_4_13_3_6.exe`
   - Default install path: `C:\Program Files\Mellanox\WinMFT`

3. After installation, the WinMFT folder contains:
   - `mst.exe`
   - `flint.bat`
   - `flint_ext.exe`
   - `mlxfwmanager.exe`
   - `mlxburn.exe`
   - `mlxconfig.exe`
   - Various DLLs and support files

> [!IMPORTANT]
> `mstflint.exe` does **not** exist as a standalone binary in this Windows install. Use `flint.bat` (which calls `flint_ext.exe`) for all flint commands. If you see guides referencing `mstflint`, substitute `.\flint.bat` instead.

### Step 1: Discover the Device

Open **PowerShell as Administrator**:

```powershell
cd “C:\Program Files\Mellanox\WinMFT”
.\mst.exe status -v
```

Expected output:

```
MST devices:
------------
  mt4099_pci_cr0         bus:dev.fn=0a:00.0
  mt4099_pciconf0        bus:dev.fn=0a:00.0
```

> [!TIP]
> Use `mt4099_pci_cr0` as the device path for all subsequent commands. This is the preferred path that was tested successfully. Do **not** use `pciconf0` unless you have a specific reason.

### Step 2: Query Current Firmware and MAC

```powershell
.\flint.bat -d mt4099_pci_cr0 q
```

Expected output:

```
Image type:            FS2
FW Version:            2.33.5220
FW Release Date:       29.3.2015
Product Version:       02.33.52.20
Rom Info:              type=PXE version=3.4.467
Device ID:             4099
Description:           Node             Port1            Port2            Sys image
GUIDs:                 ffffffffffffffff ffffffffffffffff ffffffffffffffff ffffffffffffffff
MACs:                                       e41d2da1b2c0     e41d2da1b2c1
VSD:
PSID:                  MT_1170110023
```

> [!NOTE]
> **Why two MACs on a single-port card?** This is normal. `flint` uses a **base MAC** and auto-assigns Port2 as base+1. Port1 is your active real NIC port. Port2 is stored in firmware metadata but not physically used. Do not panic when you see two MAC values on a single-port card.

### Step 3: Verify MAC in Windows

Run these commands to confirm the Windows-visible MAC matches Port1 from flint:

```powershell
getmac /v
```

```
Connection Name Network Adapter Physical Address    Transport Name
=============== =============== =================== ==========================================================
Ethernet        Mellanox Connec E4-1D-2D-A1-B2-C0   \Device\Tcpip_{XXXXXXXX-XXXX-XXXX-XXXX-XXXXXXXXXXXX}
```

```powershell
ipconfig /all
```

```
Ethernet adapter Ethernet:

   Description . . . . . . . . . . . : Mellanox ConnectX-3 Ethernet Adapter
   Physical Address. . . . . . . . . : E4-1D-2D-A1-B2-C0
   DHCP Enabled. . . . . . . . . . . : Yes
   IPv4 Address. . . . . . . . . . . : 192.168.1.100(Preferred)
```

```powershell
Get-NetAdapter | Format-Table Name, InterfaceDescription, MacAddress, Status, LinkSpeed
```

```
Name            InterfaceDescription                 MacAddress        Status LinkSpeed
----            --------------------                 ----------        ------ ---------
Ethernet        Mellanox ConnectX-3 Ethernet Adapter E4-1D-2D-A1-B2-C0 Up     10 Gbps
```

> [!IMPORTANT]
> Always verify that the Windows MAC and flint Port1 MAC match before proceeding.

### Step 4: Back Up Firmware Image

```powershell
.\flint.bat -d mt4099_pci_cr0 ri cx311a-backup.bin
```

This reads the full firmware image from flash memory to a local file. **Keep this backup safe** - it is your recovery path if anything goes wrong.

Make a copy for testing:

```powershell
Copy-Item .\cx311a-backup.bin .\cx311a-test.bin
```

### Step 5: Test New MAC on Image File Only (Strongly Recommended)

Before touching real hardware, test the MAC change on the backup image file. This proves the edit logic works without any risk to the card.

Choose a test MAC. For minimal-risk testing, change only the last nibble of the original:
- Original Port1: `E4:1D:2D:A1:B2:C0`
- Test Port1: `E4:1D:2D:A1:B2:C2`
- Port2 will automatically become: `E4:1D:2D:A1:B2:C3` (base+1)

Write the new MAC to the image file:

```powershell
.\flint.bat -i .\cx311a-test.bin -mac 0xE41D2DA1B2C2 sg
```

Expected output:

```
    You are about to change the Guids/Macs/Uids on the image:

                        New Values              Current Values
        Node  GUID:     ffffffffffffffff        ffffffffffffffff
        Port1 GUID:     ffffffffffffffff        ffffffffffffffff
        Port2 GUID:     ffffffffffffffff        ffffffffffffffff
        Sys.Image GUID: ffffffffffffffff        ffffffffffffffff
        Port1 MAC:          e41d2da1b2c2            e41d2da1b2c0
        Port2 MAC:          e41d2da1b2c3            e41d2da1b2c1

 Do you want to continue ? (y/n) [n] : y
Restoring signature                     - OK
```

Verify the modified image:

```powershell
.\flint.bat -i .\cx311a-test.bin q
```

```
Image type:            FS2
FW Version:            2.33.5220
FW Release Date:       29.3.2015
Product Version:       02.33.52.20
Rom Info:              type=PXE version=3.4.467
Device ID:             4099
Description:           Node             Port1            Port2            Sys image
GUIDs:                 ffffffffffffffff ffffffffffffffff ffffffffffffffff ffffffffffffffff
MACs:                                       e41d2da1b2c2     e41d2da1b2c3
VSD:
PSID:                  MT_1170110023
```

> [!NOTE]
> The image file now shows the new MAC values. This confirms the edit logic is correct before touching real hardware.

### Step 6: Flash the New MAC to the Real Card

```powershell
.\flint.bat -d mt4099_pci_cr0 -mac 0xE41D2DA1B2C2 sg
```

Expected output:

```
-W- GUIDs are already set, re-burning image with the new GUIDs ...
    You are about to change the Guids/Macs/Uids on the device:

                        New Values              Current Values
        Node  GUID:     ffffffffffffffff        ffffffffffffffff
        Port1 GUID:     ffffffffffffffff        ffffffffffffffff
        Port2 GUID:     ffffffffffffffff        ffffffffffffffff
        Sys.Image GUID: ffffffffffffffff        ffffffffffffffff
        Port1 MAC:          e41d2da1b2c2            e41d2da1b2c0
        Port2 MAC:          e41d2da1b2c3            e41d2da1b2c1

 Do you want to continue ? (y/n) [n] : y
Burning FS2 FW image without signatures - OK
Restoring signature                     - OK
```

> [!NOTE]
> The message "re-burning image with the new GUIDs" is normal - it means GUIDs were already set and are being preserved.
> **Success indicators**: `Burning FS2 FW image without signatures - OK` and `Restoring signature - OK`.

### Step 7: Reboot

```powershell
shutdown /r /t 0
```

### Step 8: Verify After Reboot

Open **PowerShell as Administrator** again:

```powershell
cd “C:\Program Files\Mellanox\WinMFT”
.\flint.bat -d mt4099_pci_cr0 q
```

```
Image type:            FS2
FW Version:            2.33.5220
FW Release Date:       29.3.2015
Product Version:       02.33.52.20
Rom Info:              type=PXE version=3.4.467
Device ID:             4099
Description:           Node             Port1            Port2            Sys image
GUIDs:                 ffffffffffffffff ffffffffffffffff ffffffffffffffff ffffffffffffffff
MACs:                                       e41d2da1b2c2     e41d2da1b2c3
VSD:
PSID:                  MT_1170110023
```

```powershell
getmac /v
```

```
Connection Name Network Adapter Physical Address    Transport Name
=============== =============== =================== ==========================================================
Ethernet        Mellanox Connec E4-1D-2D-A1-B2-C2   \Device\Tcpip_{XXXXXXXX-XXXX-XXXX-XXXX-XXXXXXXXXXXX}
```

```powershell
ipconfig /all
```

```
Ethernet adapter Ethernet:

   Description . . . . . . . . . . . : Mellanox ConnectX-3 Ethernet Adapter
   Physical Address. . . . . . . . . : E4-1D-2D-A1-B2-C2
   DHCP Enabled. . . . . . . . . . . : Yes
   IPv4 Address. . . . . . . . . . . : 192.168.1.100(Preferred)
```

```powershell
Get-NetAdapter | Format-Table Name, InterfaceDescription, MacAddress, Status, LinkSpeed
```

```
Name            InterfaceDescription                 MacAddress        Status LinkSpeed
----            --------------------                 ----------        ------ ---------
Ethernet        Mellanox ConnectX-3 Ethernet Adapter E4-1D-2D-A1-B2-C2 Up     10 Gbps
```

**Confirmed results:**
1. The MAC change succeeded permanently on the NIC firmware.
2. Windows picked up the new flashed MAC automatically - no driver or OS-level configuration needed.
3. The link remained up at 10 Gbps after the flash.
4. Repeated permanent MAC rewriting works on this ConnectX-3 setup.

### Choosing a Final MAC Address

The initial test above changed only one nibble as a minimal-risk proof of function. For a long-term MAC, it is cleaner to use a **locally administered MAC** starting with `02` instead of staying in the original Mellanox vendor range:

| | MAC |
|---|---|
| Example final MAC (Port1) | `02:11:22:33:44:55` |
| Port2 (auto-derived) | `02:11:22:33:44:56` |

```powershell
.\flint.bat -d mt4099_pci_cr0 -mac 0x021122334455 sg
```

> [!NOTE]
> Using a `02:xx:xx:xx:xx:xx` prefix marks the address as locally administered per IEEE standards, avoiding collisions with real vendor OUIs.

> [!NOTE]
> **Why `02:xx` instead of keeping the vendor OUI?** For USB NICs the general best practice is to preserve the original vendor OUI (first 3 bytes) and only change the last 3 - this avoids standing out as an unusual device in network logs. For a firmware-level flash like this, the situation is different: you are rewriting the actual base MAC in NIC firmware, not applying an OS-level override. Using a locally administered `02:xx` prefix is the IEEE-correct way to assign a self-chosen address and avoids accidentally duplicating a real Mellanox-assigned MAC that exists on another card somewhere. Both approaches work technically - choose based on your threat model.

### Quick Reference: Changing MAC Again Later

```powershell
cd “C:\Program Files\Mellanox\WinMFT”
.\flint.bat -d mt4099_pci_cr0 -mac 0xNEWMAC sg
shutdown /r /t 0
```

Verify after reboot:

```powershell
cd “C:\Program Files\Mellanox\WinMFT”
.\flint.bat -d mt4099_pci_cr0 q
getmac /v
```

Replace `0xNEWMAC` with your desired MAC in hex format (e.g., `0x021122334455`). Port2 is always derived automatically as base+1.

### Troubleshooting

1. **`mstflint` is "not recognized"**
   - On this Windows install, the relevant executables are `mst.exe`, `flint.bat`, and `flint_ext.exe` - **not** `mstflint.exe`. Use `.\flint.bat` for all flint operations.

2. **`mst status -v` shows nothing**
   - Check that the WinOF driver is installed correctly
   - Reboot the system
   - Reinstall WinOF, then reinstall WinMFT
   - Make sure you are running PowerShell **as Administrator**

3. **Card works in Windows but flint commands fail**
   - Use `mt4099_pci_cr0` as the device path, not `pciconf0`, unless you have a specific reason

4. **Do NOT use the following:**
   - `bb` (burn block) commands
   - `-ocr` flag
   - Random firmware image files from the internet
   - Crossflashing procedures
   - Low-level erase/write steps

5. **Do NOT update firmware first** if the card is already working and your goal is MAC changing. Adding a firmware update step introduces unnecessary risk for no benefit in this workflow.

6. **Always test on an image file first** (Step 5) before writing to the real device.

### Workflow Summary

| Step | Command | Purpose |
|---|---|---|
| 1 | `.\mst.exe status -v` | Discover device path |
| 2 | `.\flint.bat -d mt4099_pci_cr0 q` | Query current MAC and firmware |
| 3 | `getmac /v` | Verify Windows MAC matches |
| 4 | `.\flint.bat -d mt4099_pci_cr0 ri cx311a-backup.bin` | Back up firmware image |
| 5 | `.\flint.bat -i .\cx311a-test.bin -mac 0xNEWMAC sg` | Test MAC on image file |
| 6 | `.\flint.bat -d mt4099_pci_cr0 -mac 0xNEWMAC sg` | Flash MAC to real card |
| 7 | `shutdown /r /t 0` | Reboot |
| 8 | `.\flint.bat -d mt4099_pci_cr0 q` + `getmac /v` | Verify change persisted |

### Restoring Original Firmware from Backup

If you need to restore the original firmware image (including the original MAC), use the backup file from Step 4:

```powershell
cd "C:\Program Files\Mellanox\WinMFT"
.\flint.bat -d mt4099_pci_cr0 -i cx311a-backup.bin b
```

Then reboot:

```powershell
shutdown /r /t 0
```

> [!WARNING]
> This writes the full original firmware image back to the card. The `b` flag means "burn" - it flashes the entire image from the file to the device. After reboot, the card will have its original MAC and firmware state restored.

## Controller Storage: eFuse, EEPROM, or Flash

| Controller or device | Documented storage choices | What that means |
|---|---|---|
| Intel Ethernet controllers | Controller-specific EEPROM, flash, or integrated NVM; some fields can be locked | Use only a matching tool release and controller guide. Do not generalize one EEUPDATE result to another controller. [A] |
| Realtek RTL8125BG/BGS | Embedded OTP can replace external EEPROM; serial EEPROM and SPI flash are also supported | The same controller family can appear on boards with different storage. OTP writes are permanent. [A] |
| Realtek RTL8153B-VB | Embedded OTP can replace external 93C46/93C56/93C66 EEPROM; the controller also has an SPI flash interface | An RTL8153 label does not tell you which interface stores the finished adapter's MAC. [A] |
| Realtek RTL8156B(S)G | Embedded OTP can replace external 93C46/93C56/93C66 EEPROM; the controller also has an SPI flash interface | Confirm the tool's storage mode before any write. [A] |
| TP-Link UE300 | TP-Link documents an RTL8153 controller; the public product page does not identify the board's active MAC storage | Treat storage type as unknown until the tool reads it, and keep hardware revisions separate. [A] |
| ASIX AX88179 | Optional 93C56/93C66 serial EEPROM or embedded eFuse | EEPROM may be rewritable; eFuse is one-time programmable. [A] |
| ASIX AX88179A | Embedded eFuse for device data and external SPI flash for firmware customization | Do not assume that changing flash also replaces data stored in eFuse. [A] |
| ASIX AX88179B | Embedded eFuse for device data; optional external SPI flash for firmware customization | ASIX documents MAC customization through its eFuse programming tools. Treat it as permanent. [A] |
| Mellanox ConnectX-3 | Firmware image, as exercised by the tested workflow above | The documented backup-and-restore workflow is repeatable on the tested CX311A. [C] |
| Realtek RTL8126 | eFuse MAC; factory PGtool path with `8126EF.CFG` programs MAC, SVID/SMID, and LED | Factory provisioning only. Retail end-user rewriting is unconfirmed. Do not reuse the RTL8125 recipe. [C] |

The words `DUMP`, `READ`, or `BACKUP` do not guarantee reversibility. For OTP/eFuse, a backup records the original state but does not reset already programmed bits. [A]

**Marvell / Aquantia AQC113: firmware recovery only.** `flashUpdate2.exe` with signed agent and image files can update, reflash, and recover an AQC113. It matches images by the exact four-part PCI ID. [C] The public notes show no MAC-edit operation. [S] A successful firmware flash is not MAC-edit support. Older AQC107 DIAG results, both successes and "maximum number of MAC addresses programmed" failures, do not prove AQC113 compatibility. [C]

**NIC identifiers beyond the MAC.** A NIC can expose more than its MAC. Intel i226 `ADAPTERINFO` output shows ETrackID, firmware version, NVM version, MAC, and a serial number. In that output the serial is the MAC with `ffff` inserted, so it is derived from the MAC, not independent. Software can still read and compare it. [C] PCI vendor, device, subsystem, and revision IDs identify the controller or board class, not normally one unit. [A] Some adapters also expose PCIe VPD (Vital Product Data) fields such as part number and serial; an AQC113 FreeBSD probe shows a VPD part number. [C] Which of these fields exist is controller- and family-specific. For ConnectX-3 PSID and firmware data, see [Hardware Details](#hardware-details). If you change only the MAC, a MAC-derived serial can still show the old value or a mismatch.

## Verification Checklist

**Status:** the Windows commands are documented interfaces. **[A]** The vendor-tool and cold-power checks provide stronger device-level evidence but are not validated on every controller family. **[S]**

### 1. Check for a Windows override

```powershell
Get-NetAdapterAdvancedProperty -Name "*" -RegistryKeyword "NetworkAddress" -AllProperties
```

If the target adapter has a value, record and reset it before claiming that hardware storage changed. [A]

### 2. Compare Windows views

```powershell
Get-NetAdapter -Physical |
  Format-Table Name, InterfaceDescription, MacAddress, Status

getmac /v /fo list

Get-CimInstance Win32_NetworkAdapter |
  Where-Object PhysicalAdapter |
  Select-Object Name, PNPDeviceID, MACAddress
```

`Get-NetAdapter` and `getmac` are documented Windows interfaces for the current adapter address. Matching results across these views can help identify the intended interface, but they still report the active driver-visible address. [A]

### 3. Check with HWIDChecker

1. Run `HWIDChecker.exe` from the repository root.
2. Open the `NETWORK ADAPTERS (NIC's)` section.
3. Match the adapter by product name and PnP device ID, then record the displayed MAC.

HWIDChecker reads the current `Win32_NetworkAdapter.MACAddress` and matches the adapter GUID to the native interface table for **Permanent MAC**. If those values differ, it labels the current value **MAC Address (Overridden)**. When the native permanent address is unavailable, it uses the `NetworkAddress` registry value to detect an override and reports **Permanent MAC: Unavailable**. Supported NDIS queries add **Permanent MAC (OID)** for the exact PnP instance; disagreements are recorded in diagnostics. These are driver-visible readbacks, not proof that firmware storage changed. [A]

### 4. Read back with the vendor tool

> [!WARNING]
> This generic readback procedure has not been verified with every controller family. Use only the read-only command documented for the exact utility and revision. [S]

- Reopen the same controller-matched utility.
- Select the adapter again by hardware identity, not by list position alone.
- Use its read-only query or dump function.
- Compare the reported `NODEID` or MAC, storage mode, and remaining eFuse capacity with the pre-write record.

### 5. Cold-power persistence check

> [!WARNING]
> This general persistence test is not a vendor-specific recovery procedure and does not make a hardware write reversible. [S]

1. Shut down Windows.
2. Remove power from the NIC or unplug the USB adapter and confirm that it fully loses power.
3. Start again with no `NetworkAddress` override.
4. Repeat the Windows, HWIDChecker, and vendor-tool checks.

A changed current address after only a driver restart proves a software-visible change. A matching vendor readback after cold power is stronger, but still controller-specific, evidence of a device-storage change. [S]

## Sources

- [Microsoft: NdisReadNetworkAddress and the `NetworkAddress` registry value](https://learn.microsoft.com/en-us/windows-hardware/drivers/ddi/ndis/nf-ndis-ndisreadnetworkaddress)
- [Microsoft: NDIS current and permanent MAC address attributes](https://learn.microsoft.com/en-us/windows-hardware/drivers/ddi/miniportgeneralattributes/ns-miniportgeneralattributes-ndis_miniport_adapter_general_attributes)
- [Microsoft: Set-NetAdapter](https://learn.microsoft.com/en-us/powershell/module/netadapter/set-netadapter?view=windowsserver2025-ps)
- [Microsoft: Get-NetAdapter](https://learn.microsoft.com/en-us/powershell/module/netadapter/get-netadapter?view=windowsserver2025-ps)
- [Microsoft: Get-NetAdapterAdvancedProperty](https://learn.microsoft.com/en-us/powershell/module/netadapter/get-netadapteradvancedproperty?view=windowsserver2025-ps)
- [Microsoft: Reset-NetAdapterAdvancedProperty](https://learn.microsoft.com/en-us/powershell/module/netadapter/reset-netadapteradvancedproperty?view=windowsserver2025-ps)
- [Microsoft: getmac](https://learn.microsoft.com/en-us/windows-server/administration/windows-commands/getmac)
- [Microsoft: ipconfig](https://learn.microsoft.com/en-us/windows-server/administration/windows-commands/ipconfig)
- [Microsoft: Win32_NetworkAdapter](https://learn.microsoft.com/en-us/windows/win32/cimwin32prov/win32-networkadapter)
- [IEEE: Guidelines for EUI, OUI, CID, and local-address bits](https://standards.ieee.org/wp-content/uploads/import/documents/tutorials/eui.pdf)
- [TP-Link: UE300 product specifications](https://www.tp-link.com/us/home-networking/usb-converter/ue300/)
- [TP-Link: UE300 hardware-version support page](https://www.tp-link.com/us/support/download/ue300/)
- [TP-Link: UE300 V1 datasheet](https://static.tp-link.com/res/down/doc/UE300_V1_Datasheet.pdf)
- [Realtek: RTL8153B-VB-CG](https://www.realtek.com/Product/Index?id=4078)
- [Realtek: RTL8156B(S)G-CG](https://www.realtek.com/Product/Index?cate_id=786&id=3967&menu_id=384)
- [Realtek: RTL8125BG(S)-CG](https://www.realtek.com/Product/Index?cate_id=786&id=3962)
- [ASIX: AX88179 storage and programming FAQ search](https://www.asix.com.tw/en/search?keyword=AX88179)
- [ASIX: AX88179A product page and programming FAQ](https://www.asix.com.tw/en/product/USBEthernet/Super-Speed_USB_Ethernet/AX88179A)
- [ASIX: AX88179B product page and programming FAQ](https://www.asix.com.tw/en/product/USBEthernet/Super-Speed_USB_Ethernet/AX88179B)
- [ASIX: AX88179A mass-production storage guidance](https://www.asix.com.tw/en/application/SmartHome/Best_Practices_for_Designing_USB32_Gigabit_Ethernet_Solution)
- [Intel: Ethernet Controller Products 30.4 release notes](https://cdrdv2-public.intel.com/864646/Intel%20Ethernet%20Controller%20Products_Release%20Notes_30.4_v2.pdf)
- [Intel: Ethernet Controller Products 25.2 release notes](https://cdrdv2-public.intel.com/630597/630597%20-%20Software_Release_25_2_v_1_1_External.pdf)
- [Intel: Ethernet Adapters and Devices User Guide](https://edc.intel.com/output/DownloadPdfDocument?id=10427)
- [HWID-Privacy: Rust network provider](../../app/rust/src/hw/network.rs)
- [Intel Community: i226 EEUPDATE ADAPTERINFO output (ETrackID, NVM, serial)](https://community.intel.com/t5/Ethernet-Products/i226-LM-and-i-226V-NIC-NVM-cannot-be-updated-on-DFI-motherboard/m-p/1672867)
- [KevinYSH: Realtek LAN chip PGtool user guide (UEFI), RTL8126 `8126EF.CFG`](https://github.com/KevinYSH/document/blob/master/DE-LDRET004_Realtek_LAN_Chip_PGtool_UserGuide_UEFI.md)
- [NVIDIA Developer Forums: RTL8126 MAC from eFuse](https://forums.developer.nvidia.com/t/technical-inquiry-regarding-mac-address-provisioning-for-custom-jetson-carrier-board-with-multi-nic-aqr113c-dp83867-rtl8126/368972)
- [coronas2k: AQC113 flashUpdate2 firmware update and recovery notes](https://gist.github.com/coronas2k/c7d3a37ca04da2f15783da3cc8cf3702)
- [InsanelyMac: Aquantia AQC107 DIAG thread](https://www.insanelymac.com/forum/topic/330614-marvell-aquantia-10-gb-ethernet-support-thread/page/9/) and [ndoo.sg: Aquantia homelab notes](https://ndoo.sg/projects%3Ahomelab%3Aaquantia)
- [FreeBSD freebsd-net list: AQC113 probe with PCIe VPD](https://lists.freebsd.org/archives/freebsd-net/2025-October/007734.html)
