# **MOBO SPOOFING GUIDE**

> [!CAUTION]
> This guide changes firmware-provided identity data. A wrong utility, unsupported command, interrupted write, or invalid firmware image can leave the board unable to boot. **[A]** Save BitLocker recovery keys. Record the exact motherboard model and revision. Prepare the manufacturer's documented recovery method before writing anything.
>
> Evidence grades used below: **[C]** means confirmed first-hand by a named user with details. **[A]** means verified against a cited primary source. **[CC]** means supported by multiple independent community reports. **[S]** means a single or otherwise unverified claim.
>
> Motherboard writing is untested. The AMIDEWIN/DMIEdit steps and command switches remain **[S]** unless a board vendor documents them for the exact model.

> [!WARNING]
> The bundled archives are not official vendor downloads. Their provenance, signatures, and compatibility are unverified. **[S]** Do not assume that a utility is safe merely because it starts successfully or can read the current values.

## Table of contents

- [Prerequisites](#prerequisites)
- [Instructions](#instructions)
- [Important notes](#important-notes)
- [What this changes](#what-this-changes)
- [Requirements and recovery preparation](#requirements-and-recovery-preparation)
- [AMIDEWIN and DMIEdit workflow notes](#amidewin-and-dmiedit-workflow-notes)
- [ASUS-specific procedure boundary](#asus-specific-procedure-boundary)
- [Verify with HWIDChecker](#verify-with-hwidchecker)
- [Troubleshooting](#troubleshooting)
- [Sources](#sources)
 
## **Prerequisites**
 
- Recommended way:
  - DMI EDIT WIN64 FILES:
    - [dmi-edit-win64-ami.zip](./tools/dmi-edit-win64-ami.zip)
- Optional:
  - [DMIEDIT GUI v5.27.05.0016 (latest).zip](<./tools/DMIEDIT GUI v5.27.05.0016 (latest).zip>)
  - [dmmiedit GUI (not working on new mobos).zip](<./tools/dmmiedit GUI (not working on new mobos).zip>)
    - This version does not work properly on newer mobos or in general, I'd suggest using the one above!
  - **[HWIDChecker.exe](/HWIDChecker.exe)**
    - (Optional but recommended checking your before/after SSD details)

---

> [!WARNING]
> The following owner-authored workflow is **[C]** as a report of the owner's procedure. It has not been independently repeated. Compatibility of the bundled utility and each write command with any other board or firmware remains **[S]**.

## **Instructions**

### Step 1: Extract Current Serial Numbers
1. Extract the `dmi-edit-win64-ami.zip` contents to a folder
2. Run `1.GET ALL SERIALS.bat` as Administrator
3. This will create a timestamped text file with all current serial numbers
4. Note down your current:
   - System UUID
   - Baseboard Serial Number
   - Baseboard Name

### Step 2: Modify Serial Numbers
1. Open `2.CHANGE SERIALS EXAMPLE DONT RUN.bat` in a text editor
2. Follow these guidelines for modifications:
   - Change only 2-5 digits of your original serial
   - Avoid odd patterns (e.g., `SPOOFER-XXXX`)
   - Example:
     - Original: `08ZU9T1_NAVX2ZXV4F`
     - Changed: `08ZU9T1_NABX12XZ4A`
3. Update the commands in the batch file with your new values:
   - `/SU` - System UUID (generate a new UUID)
   - `/BS` - Baseboard Serial Number
   - `/BP` - Baseboard Name (optional)

> [!WARNING]
> The two owner-original values above look realistic, but their provenance is not established. The owner must confirm that both are fabricated before publication.

### Step 3: Apply Changes
1. Run the modified `2.CHANGE SERIALS.bat` as Administrator
2. The tool will update the DMI/BIOS information

### Step 4: Final Steps
1. Reflash your BIOS to make changes permanent
2. Clear CMOS after flashing
3. Verify changes using HWIDChecker.exe

## **Important Notes**
- Always backup your original serial numbers
- Changes may require BIOS reflash to persist
- Some motherboards may have additional protection - check your manufacturer's documentation
- For MSI motherboards, see `MSI AMIDEINx64 spoof befehle cmd.rtf` for additional commands

## What this changes

SMBIOS defines structures that platform firmware uses to expose system-management data. It is not one serial number. The DMTF specification separates the relevant values into several structures. **[A]**

| SMBIOS structure | Fields relevant to this guide | Common AMIDEWIN switch reported by the source | Evidence |
|---|---|---|---|
| Type 1, System Information | Manufacturer, product, version, system serial, UUID, SKU, family | `/SM`, `/SP`, `/SV`, `/SS`, `/SU`, `/SK`, `/SF` | Fields **[A]**; switch mapping **[S]** |
| Type 2, Baseboard Information | Manufacturer, product, version, board serial, asset tag, location | `/BM`, `/BP`, `/BV`, `/BS`, `/BT`, `/BLC` | Fields **[A]**; switch mapping **[S]** |
| Type 3, Chassis Information | Manufacturer, version, chassis serial, asset tag, SKU | `/CM`, `/CV`, `/CS`, `/CA`, `/CSK` | Fields **[A]**; switch mapping **[S]** |
| Type 11, OEM Strings | Free-form strings defined by the OEM | `/OS` | Structure **[A]**; switch mapping and indexing **[S]** |

The UUID is a 16-byte Type 1 field. SMBIOS 2.6 clarified that the first three UUID fields use little-endian byte order. Compatibility tools often interpret older tables without that byte swap because older firmware was inconsistent. A formatted UUID can therefore look different from the same 16 bytes in a raw firmware dump. **[A]**

Do not search for or replace UUID bytes in a ROM until you have checked the reported SMBIOS version and the tool's representation.

> [!NOTE]
> The manufacturer and product fields describe the system or board. They are separate from the serial-number fields. Preserve the real manufacturer and product name unless you are correcting a genuine firmware error. **[A]**

DMTF confirms that SMBIOS Type 4 can contain a processor serial number, asset tag, and part number. It says the processor serial and part number are set by the manufacturer and are normally not changeable. **[A]**

These are not motherboard serial fields. This guide deliberately excludes them from its write workflow.

## Requirements and recovery preparation

Before any write:

> [!WARNING]
> The recovery procedure is untested. A saved firmware dump is evidence, but whether it is a usable restore image for a specific board remains **[S]**.

1. Record the exact motherboard model, board revision, current BIOS version, and current firmware settings.
2. Export or photograph the BitLocker recovery key. If a firmware or TPM change is planned, suspend BitLocker using Microsoft's documented procedure and resume it after the machine boots normally. **[A]**
3. Save the complete output from `HWIDChecker.exe`. Also keep the timestamped serial export created by the existing Step 1.
4. Download a stock recovery BIOS only from the motherboard vendor. Match the exact model and follow any revision-specific instructions from that vendor. **[A]**
5. Read the model-specific recovery instructions before starting. A saved ROM dump is useful evidence, but it is not automatically a usable recovery image. **[S]**
6. Use stable power. During an official update, do not disconnect power or interrupt the process. **[A]**

Useful read-only Windows checks are:

```powershell
Get-CimInstance Win32_ComputerSystemProduct |
  Select-Object Vendor, Name, IdentifyingNumber, UUID

Get-CimInstance Win32_BaseBoard |
  Select-Object Manufacturer, Product, Version, SerialNumber

Get-CimInstance Win32_SystemEnclosure |
  Select-Object Manufacturer, Version, SerialNumber, SMBIOSAssetTag
```

Microsoft documents the UUID as an SMBIOS Type 1 value, the baseboard serial through `Win32_BaseBoard`, and the chassis serial through `Win32_SystemEnclosure`. **[A]**

## AMIDEWIN and DMIEdit workflow notes

AMI confirms that AFU sends update requests that the system firmware processes. **[A]** The linked AMI pages do not document the exact AMIDEWIN/DMIEdit command set used by the bundled archives. Exact compatibility therefore remains **[S]**.

> [!WARNING]
> The following tool-specific guidance is untested. Treat every AMIDEWIN switch and every write as **[S]**. Stop if the utility reports an unsupported function, driver error, write protection, secure-flash rejection, or a mismatch between the current board and the selected tool build.

Use the narrowest possible workflow:

1. Run the read-only serial collection first.
2. Open the example batch file in a text editor. Inspect every command. Never run a file named `EXAMPLE DONT RUN` unchanged.
3. Change one identity class at a time. Keep product, vendor, version, and family fields consistent with the physical board.
4. Prefer the existing `/SU` and `/BS` scope unless a specific additional field has a documented reason to change.
5. Reboot, collect the values again, and compare the result with the saved baseline before attempting another write.
6. Do not repeatedly retry a rejected write. A protection failure is not proof that a different utility version is safe.

The automatic manager offers to make a backup and prints the AMIDEWIN commands for manual review before execution. Treat the manager and its generated commands as unverified. **[S]**

Keep using the local read-only collection batch. Review the change batch line by line. Retain the before-and-after output.

## ASUS-specific procedure boundary

ASUS USB BIOS FlashBack is a board-specific recovery/update feature. ASUS requires the BIOS file for the exact board model, the correct filename, the dedicated USB port, and uninterrupted power until the FlashBack light goes out. **[A]** The official instructions describe vendor firmware. They do not validate a modified ROM image.

> [!WARNING]
> **[S] Untested procedure:** Dump the ROM with `AFUWINx64.exe DUMP.rom /O` and edit only a copy. The proposed method replaces the 16 UUID bytes without changing file size, flashes the modified image, and uses AMIDEWIN for other SMBIOS fields. Do not treat successful dumping or editing as proof that the image is safe to flash.

If you research that path on an ASUS board:

1. Confirm that the exact board model supports USB BIOS FlashBack in its official manual. **[A]**
2. Prepare the official stock recovery image and verify the required vendor filename and FlashBack port. **[A]**
3. Keep the original dump unchanged. Work only on a copy. **[S]**
4. Compare the original and edited image byte-for-byte. The proposed UUID replacement must remain exactly 16 bytes and must not change the file size. This does not validate checksums, signatures, layout, or flash safety. **[S]**
5. Do not flash a modified image unless its compatibility, integrity checks, recovery path, and exact board support have been independently verified. **[S]**
6. During an official FlashBack operation, do not remove the USB drive, disconnect power, turn the system on, or press Clear CMOS. ASUS warns that interruption can prevent boot. **[A]**

Do not force-downgrade ASUS firmware as a general SMBIOS-writing procedure. No cited official source validates that approach. **[S]**

## Verify with HWIDChecker

Run `HWIDChecker.exe` before the change, immediately after the first reboot, and again after a full shutdown and cold boot.

Compare these sections:

- **(SM)BIOS**: `UUID`, `System Serial`, `System Manufacturer`, and the reported BIOS version.
- **MOTHERBOARD**: `Manufacturer`, `Product`, `Version`, and `SerialNumber`.
- **CHASSIS**: `Manufacturer`, `Version`, `Serial Number`, and `Asset Tag`.

HWIDChecker reads raw SMBIOS data and uses WMI where needed. **[A]** Compare its output with the read-only CIM commands as a consistency check. Both views can originate from the same firmware data, so this is not independent proof. **[A]** Keep both before-and-after exports private.

> [!NOTE]
> A changed field proves only that the displayed field changed. It does not prove that every firmware table, vendor service, management controller, TPM identity, or EFI variable changed. Inspect EFI variables separately in the [NVRAM and EFI variable guide](../nvram-spoofing/nvram-spoofing.md#overview).

## Troubleshooting

> [!WARNING]
> The cases marked **[S]** below are diagnostic possibilities. They are not tested recovery procedures.

### A value changes and then returns after reboot

The write may have affected only a runtime copy, or the firmware may rebuild the table from protected data. Persistence is board- and firmware-specific. **[S]** Restore the saved baseline information and consult the board vendor. Do not loop the same write command.

### The UUID differs between a ROM editor and Windows

For SMBIOS 2.6 and later, compare the raw 16 bytes using the specification's little-endian rule for the first three UUID fields. Older firmware may need the legacy interpretation used by compatibility tools. **[A]**

### The board shows `To Be Filled By O.E.M.` or an empty serial

That can be an OEM placeholder. It is not evidence that a write succeeded. **[S]** Save the value exactly as reported. Do not invent a manufacturer or model string to fill it.

### A firmware update restores old values

An official update may replace or regenerate SMBIOS data. **[S]** Do not assume that reflashing makes edits permanent. Verify after every firmware update with HWIDChecker and the CIM commands.

### The system no longer boots

Stop further writes. Use only the exact board vendor's documented recovery method and the stock image prepared before the change. If BitLocker requests recovery, use the saved recovery key. Do not clear additional EFI variables as a troubleshooting guess.

## Sources

- [DMTF SMBIOS Specification 3.9.0](https://www.dmtf.org/sites/default/files/standards/documents/DSP0134_3.9.0.pdf)
- [Linux kernel: SMBIOS UUID version handling](https://github.com/torvalds/linux/blob/master/drivers/firmware/dmi_scan.c)
- [Microsoft: Win32_ComputerSystemProduct](https://learn.microsoft.com/en-us/windows/win32/cimwin32prov/win32-computersystemproduct)
- [Microsoft: Win32_BaseBoard](https://learn.microsoft.com/en-us/windows/win32/cimwin32prov/win32-baseboard)
- [Microsoft: Win32_SystemEnclosure](https://learn.microsoft.com/en-us/windows/win32/cimwin32prov/win32-systemenclosure)
- [Microsoft: suspend BitLocker for non-Microsoft firmware updates](https://learn.microsoft.com/en-us/troubleshoot/windows-client/windows-security/suspend-bitlocker-protection-non-microsoft-updates)
- [AMI: AFU for Aptio V](https://www.ami.com/resources/ami-firmware-utility-afu-a-secure-update-utility-for-aptio-v-uefi-bios-firmware/)
- [ASUS: How to use USB BIOS FlashBack](https://www.asus.com/support/faq/1038568/)
- [Fundryi/HWID-Privacy HWIDChecker source](https://github.com/Fundryi/HWID-Privacy/tree/main/app/src/Hardware)
