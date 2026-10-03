# NVRAM and EFI Variable Privacy Guide

> [!CAUTION]
> EFI variables are firmware configuration data. Deleting the wrong variable can remove boot entries, break Secure Boot, trigger BitLocker recovery, or leave firmware unable to POST. **[A]** The Linux kernel makes many non-standard variables immutable because real firmware has failed to POST after their removal. **[A]**

This guide separates standard boot variables from identifier-like, vendor-specific variables. It provides a read-only inspection workflow. It does not provide a mass-deletion recipe.

Evidence grades used here: **[C]** means confirmed first-hand by a named user with details. **[A]** means verified against a cited primary source. **[CC]** means supported by multiple independent community reports. **[S]** means a single or otherwise unverified claim.

Writing or deleting EFI variables is untested.

## Table of contents

- [Overview](#overview)
- [What the variables mean](#what-the-variables-mean)
- [Requirements](#requirements)
- [Backup and recovery preparation](#backup-and-recovery-preparation)
- [Read-only inspection steps](#read-only-inspection-steps)
- [Why this guide does not delete variables](#why-this-guide-does-not-delete-variables)
- [Verify with HWIDChecker](#verify-with-hwidchecker)
- [Troubleshooting](#troubleshooting)
- [Sources](#sources)

## Overview

UEFI firmware stores named variables. A variable is identified by its name and vendor GUID. Its attributes control access and whether it survives a power cycle. Only variables with the `EFI_VARIABLE_NON_VOLATILE` attribute are stored in nonvolatile storage. **[A]**

Windows exposes firmware-variable APIs. Linux exposes variables through `efivarfs`. **[A]**

Microsoft documents that its firmware-variable APIs fail on legacy BIOS systems. They also fail when Windows was installed in legacy BIOS mode. Access requires the system-environment privilege. **[A]** Administrator status alone does not establish that an unknown variable is safe to read, write, or delete.

NVRAM is not the same thing as SMBIOS. SMBIOS reports structured system, baseboard, chassis, and OEM data. EFI variables hold boot configuration, security databases, firmware settings, and platform-specific data. **[A]** See the [motherboard guide](../motherboard-spoofing/motherboard-spoofing.md#what-this-changes).

Some firmware may use EFI variables as a backing store for SMBIOS values, but the variable name alone does not prove that relationship. **[S]**

## What the variables mean

### Standard boot variables

`BootOrder` contains an ordered list of boot-option numbers. Each `Boot####` variable contains one boot load option. The four hexadecimal digits can range from `0000` to `FFFF`; they are not fixed to `Boot0001` through `Boot0006`. **[A]**

These variables can contain a description and a device path. They are boot configuration, not motherboard serial numbers. Deleting them can make an operating system or recovery environment disappear from the firmware boot menu. **[A]**

`BootCurrent` records the option used for the current boot. `BootNext` selects one option for the next boot, and compliant firmware deletes `BootNext` before transferring control to that option. A standard variable changing or disappearing can therefore be expected behavior rather than evidence of an identity change. **[A]**

### Identifier-like variables reported in third-party research

The following names appear in a cited third-party reverse-engineering report. No cited primary source establishes the listed identity meanings. Treat them as **[S]**.

| Variable pattern | Reported role | Evidence and boundary |
|---|---|---|
| `OfflineUniqueIDRandomSeed-*` | Random seed used as one possible source for a Windows offline device identifier | Third-party reverse engineering only **[S]** |
| `OfflineUniqueIDRandomSeedCRC-*` | Check value associated with the random-seed variable | Third-party reverse engineering only **[S]** |
| `OfflineUniqueIDEKPub-*` | Cached TPM endorsement public-key material used as another offline-ID source | Third-party reverse engineering only **[S]** |
| `OfflineUniqueIDEKPubCRC-*` | Check value associated with the cached public-key variable | Third-party reverse engineering only **[S]** |
| `UnlockID-*` and `UnlockIDCopy-*` | Purpose is undocumented; third-party research treats them as identity-related | No authoritative identity mapping found **[S]** |
| `DmiVar-*` | Firmware-specific DMI or SMBIOS backing data on some platforms | Name and layout are vendor-specific **[S]** |
| `MacAddrVar-*` | Firmware-specific data that may contain network-controller configuration on some platforms | Name alone does not prove it contains the active MAC address **[S]** |

One reverse-engineering report places the `OfflineUniqueID*` variables in namespace `{eaec226f-c9a3-477a-a826-ddc716cdc0e3}` and describes the seed as 32 bytes. This has not been confirmed by Microsoft documentation and remains **[S]**.

> [!WARNING]
> Do not publish raw variable contents or stable hashes. They may contain device-specific data, keys, certificates, paths, or identifiers. **[S]**

## Requirements

- A system booted in native UEFI mode.
- A Linux live environment booted in UEFI mode for read-only `efivarfs` inspection.
- An external drive for the local evidence backup.
- The exact motherboard manual and its documented firmware-recovery procedure.
- The BitLocker recovery key for every encrypted drive.
- [HWIDChecker.exe](/HWIDChecker.exe) for the adjacent SMBIOS baseline.

No third-party EFI-variable editor is required for the inspection workflow.

## Backup and recovery preparation

1. Save BitLocker recovery keys. If you plan any firmware write rather than the read-only inspection below, suspend BitLocker first and resume it after the system boots normally. Microsoft documents that firmware and boot-component changes can trigger recovery. **[A]**
2. Record the current boot order in the firmware setup screen.
3. Prepare the motherboard vendor's stock recovery image and recovery instructions.
4. Run HWIDChecker and save the **(SM)BIOS**, **MOTHERBOARD**, and **CHASSIS** sections.
5. Boot the Linux live environment in UEFI mode.

Create a local evidence copy of the variable files on an external drive. Replace the destination path with the mounted external drive:

```bash
sudo tar --xattrs --acls -cpf \
  /path/to/external-drive/efivars-$(date +%Y%m%d-%H%M%S).tar \
  -C /sys/firmware/efi efivars
```

> [!WARNING]
> Keep this archive private because it contains raw variable data. It is an evidence backup, not a promised one-command restore. Authenticated variables, firmware policy, write attributes, and vendor checks can prevent restoration. The restore path is unverified. **[S]**

## Read-only inspection steps

### 1. Confirm that the live environment booted through UEFI

```bash
test -d /sys/firmware/efi || {
  echo "Not booted through UEFI"
  exit 1
}
```

### 2. Mount `efivarfs` only if needed

The Linux kernel documents this mount point and filesystem type. **[A]**

```bash
mountpoint -q /sys/firmware/efi/efivars || \
  sudo mount -t efivarfs none /sys/firmware/efi/efivars
```

### 3. Save a names-only inventory

```bash
sudo find /sys/firmware/efi/efivars \
  -maxdepth 1 -type f -printf '%f\n' | sort \
  > /path/to/external-drive/efivar-names.txt
```

The filename format is the variable name followed by its vendor GUID. **[A]** Do not infer a variable's contents from the name alone.

### 4. Record sizes and local hashes for selected variables

```bash
shopt -s nullglob

patterns=(
  OfflineUniqueIDRandomSeed
  OfflineUniqueIDRandomSeedCRC
  OfflineUniqueIDEKPub
  OfflineUniqueIDEKPubCRC
  UnlockID
  UnlockIDCopy
  DmiVar
  MacAddrVar
)

for pattern in "${patterns[@]}"; do
  for file in /sys/firmware/efi/efivars/"$pattern"-*; do
    sudo stat --printf='%n %s bytes\n' "$file"
    sudo sha256sum "$file"
  done
done
```

Use the hash only for a local before-and-after comparison. Keep the output private.

### 5. Understand raw dumps before reading them

Linux prepends four little-endian attribute bytes to each `efivarfs` file. The bytes after that prefix are the variable payload. **[A]**

```bash
sudo xxd -g 1 -l 68 \
  /sys/firmware/efi/efivars/OfflineUniqueIDRandomSeed-*
```

Keep this output private. If the wildcard matches more than one namespace, inspect the filename and GUID before drawing conclusions.

## Why this guide does not delete variables

Do not remove identifier-like variables, selected `Boot0001` through `Boot0006`, or `DmiVar-*` as a general privacy procedure.

- `Boot####` numbers are allocated boot options, not a universal privacy list. Removing an arbitrary range can delete the active Windows, Linux, recovery, or network boot entry. **[A]**
- The Linux kernel marks many non-standard variables immutable because deleting them has caused firmware to fail to POST. Running `chattr -i` removes that safety barrier. **[A]**
- `DmiVar-*` and `MacAddrVar-*` have no portable public schema. A pattern that is relevant on one board may have a different purpose or be absent on another. **[S]**
- Windows firmware-variable writes require the system-environment privilege, the correct namespace GUID, the correct attributes, and firmware support. Administrative access does not make an unknown write safe. **[A]**
- A variable being recreated after boot does not by itself prove an identity role. The UEFI specification allows firmware to add or remove boot options and requires it to update some standard boot variables. **[A]**

> [!CAUTION]
> There is no verified wildcard deletion procedure in this guide. If a motherboard or operating-system vendor publishes a model-specific removal or reset procedure, follow that exact procedure and its recovery instructions. Otherwise, stop at read-only inspection.

## Verify with HWIDChecker

HWIDChecker currently displays SMBIOS, motherboard, chassis, TPM, storage, network, and other hardware information. It does not enumerate arbitrary EFI variables. **[A]** Therefore:

1. Use HWIDChecker before and after any vendor-supported firmware service to confirm whether the **(SM)BIOS**, **MOTHERBOARD**, or **CHASSIS** values changed.
2. Use the private `sha256sum` output to determine whether a selected EFI variable changed.
3. Do not claim that the hardware identity changed solely because one EFI-variable hash changed.
4. Do not claim that NVRAM is unchanged solely because HWIDChecker output is unchanged.

This separates two different evidence sources instead of treating either one as complete.

## Troubleshooting

> [!WARNING]
> The cases marked **[S]** below are diagnostic possibilities. They are not tested recovery procedures.

### `/sys/firmware/efi` does not exist

A common cause is that the live environment booted in legacy or CSM mode. Other causes include missing kernel EFI support or unavailable firmware interfaces. **[S]** Reboot and select the USB entry prefixed with `UEFI`. Do not create the directory manually.

### `efivarfs` is not mounted

Use the documented mount command above. If the mount fails, stop and record the error. Do not switch to an EFI-variable write utility as a workaround.

### A reported variable is absent

Variable availability is firmware- and Windows-version-specific. Absence is not an error and is not proof of a privacy change. Do not create a missing variable. **[S]**

### A hash changes after booting Windows

The variable may contain runtime-maintained state. A changed hash does not identify which bytes changed or why. Keep the before-and-after files private and seek a documented schema before interpreting the difference. **[S]**

### A boot option disappeared

Use the firmware boot menu or the motherboard vendor's documented recovery method. If a deleted variable caused the problem, avoid deleting anything else. The evidence archive may help diagnosis, but restoration is unverified. **[S]**

### BitLocker requests recovery

Use the saved recovery key. After the machine boots normally and the firmware state is stable, resume BitLocker if it was suspended. **[A]** Do not clear the TPM or delete more EFI variables to bypass the prompt.

## Sources

- [UEFI Specification 2.11: Boot Manager](https://uefi.org/specs/UEFI/2.11/03_Boot_Manager.html)
- [UEFI Specification 2.11: Variable Services](https://uefi.org/specs/UEFI/2.11/08_Services_Runtime_Services.html)
- [Linux kernel: efivarfs](https://docs.kernel.org/filesystems/efivarfs.html)
- [Microsoft: GetFirmwareEnvironmentVariableW](https://learn.microsoft.com/en-us/windows/win32/api/winbase/nf-winbase-getfirmwareenvironmentvariablew)
- [Microsoft: SetFirmwareEnvironmentVariableExW](https://learn.microsoft.com/en-us/windows/win32/api/winbase/nf-winbase-setfirmwareenvironmentvariableexw)
- [Microsoft: suspend BitLocker for non-Microsoft firmware updates](https://learn.microsoft.com/en-us/troubleshoot/windows-client/windows-security/suspend-bitlocker-protection-non-microsoft-updates)
- [DMTF SMBIOS Specification 3.9.0](https://www.dmtf.org/sites/default/files/standards/documents/DSP0134_3.9.0.pdf)
- [Third-party reverse engineering of Windows offline device ID](https://iretq.com/inside-getofflinedeviceuniqueid-how-windows-derives-its-offline-device-id/) **[S]**
- [Fundryi/HWID-Privacy HWIDChecker source](https://github.com/Fundryi/HWID-Privacy/tree/main/app/src/Hardware)
