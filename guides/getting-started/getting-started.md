# Getting Started: Hardware Identity Privacy Fundamentals

> [!NOTE]
> This page is the map for the whole project: what an HWID is, which parts expose identity, the order of work, and how to prove a change.
> The page itself changes no identifier. The linked part guides do that.
> Risk lives in the part guides: firmware and device writes can brick hardware or void warranties.
> This project covers privacy research, inventory validation, and education. It does not promise anonymity or unlinkability.
> A changed value in one tool proves only that the displayed value changed.

Hardware identity is not one number. A Windows PC exposes a collection of identifiers from component firmware, platform firmware, the operating system, file systems, drivers, and the local network. Privacy research starts by measuring those layers separately. **[A]**

## How to read these guides

Every guide states what a change does, what it bypasses, who reads the identifier, and how to verify the result.

Evidence grades appear inline next to claims:

- **[C]** confirmed first hand by a named user with hardware and procedure details.
- **[A]** verified against a cited specification, vendor document, Microsoft document, or repository source.
- **[CC]** supported by multiple independent community reports.
- **[S]** a single claim or a procedure that has not been independently verified.

Each procedure carries a status line naming who tested it. Risk boxes mark permanent or destructive operations. Treat anything graded **[S]** as a research lead, not a procedure.

## Table of Contents

- [What an HWID is](#what-an-hwid-is)
- [Identifier groups](#identifier-groups)
- [Persistent and software-level identifiers](#persistent-and-software-level-identifiers)
- [Plan the work in the right order](#plan-the-work-in-the-right-order)
- [Device restrictions](#device-restrictions)
- [Safety checklist](#safety-checklist)
- [Take before and after snapshots](#take-before-and-after-snapshots)
- [Clean Windows reinstall checklist](#clean-windows-reinstall-checklist)
- [Known-spoofable hardware](#known-spoofable-hardware)
- [Reported anti-cheat status](#reported-anti-cheat-status)
- [Sources](#sources)

## What an HWID is

`HWID` is shorthand for the set of values that software can use to describe or correlate a computer. There is no single universal HWID shared by every application. One program may use the SMBIOS system UUID and disk serial. Another may add a TPM endorsement identity, NIC MAC address, monitor EDID, USB serials, or Windows registry values. **[A]**

The main layers are:

1. **Platform firmware:** SMBIOS system, baseboard, firmware, and chassis data. DMTF defines separate SMBIOS structures for system information, baseboard information, and chassis information. **[A]** [DMTF SMBIOS 3.10.0](https://www.dmtf.org/sites/default/files/standards/documents/DSP0134_3.10.0.pdf)
2. **Device firmware:** Storage serials and namespace IDs, a NIC's permanent MAC, RAM SPD data, monitor EDID, GPU firmware data, and USB device serial strings.
3. **Security hardware:** TPM keys and certificates. A TPM endorsement public key, its hash, an EK certificate, the certificate serial, and an attestation key are different values. **[A]** [TCG EK Credential Profile](https://trustedcomputinggroup.org/wp-content/uploads/TCG-EK-Credential-Profile-for-TPM-Family-2.0-Level-0-Version-2.7_Pub.pdf)
4. **Windows state:** The registry value commonly called MachineGuid, hardware-profile GUID, Windows Product ID, product key when firmware exposes one, install date, PnP device instances, and driver configuration.
5. **File-system state:** GPT and partition identifiers plus volume serial numbers. A volume serial is not the manufacturer's disk serial. **[A]** [Microsoft GetVolumeInformationW](https://learn.microsoft.com/en-us/windows/win32/api/fileapi/nf-fileapi-getvolumeinformationw)
6. **Network state:** Current MAC addresses, software MAC overrides, Bluetooth radio addresses, and dynamic ARP entries that map nearby IP addresses to link-layer addresses.

Windows uses hardware IDs and device instance IDs to enumerate devices and select drivers. A device instance can include a serial supplied by the bus or device. **[A]** [Microsoft hardware IDs](https://learn.microsoft.com/en-us/windows-hardware/drivers/install/hardware-ids), [Microsoft device instance IDs](https://learn.microsoft.com/en-us/windows-hardware/drivers/install/device-instance-ids)

HWIDChecker reads MachineGuid from `HKLM\SOFTWARE\Microsoft\Cryptography`. **[A]** A Microsoft-hosted Q&A answer describes it as generated during Windows installation, but Microsoft does not document it there as a hardware serial or a privacy boundary. Treat it as installation state, not as a guaranteed unique or stable hardware identifier. **[S]** [Microsoft Q&A: choosing a machine identifier](https://learn.microsoft.com/en-us/answers/questions/5762504/unique-id-of-machine)

> [!CAUTION]
> Do not treat a blank, zero, placeholder, or changed field as proof that the computer has a new identity. Other interfaces may still expose the original component, and a remote service may combine hardware values with accounts, network data, or behavior.

## Identifier groups

The repository-root `HWIDChecker.exe` is an inspector. It does not spoof identifiers. Its current source runs 14 providers in parallel and keeps this display order. **[A]** [HardwareInfoManager source](https://github.com/Fundryi/HWID-Privacy/blob/main/app/src/Hardware/HardwareInfoManager.cs)

| HWIDChecker section | What it displays | Main layer |
|---|---|---|
| **DISK DRIVES** | Physical model and serial, firmware, hardware ID, storage UniqueId values, partition identity, drive letter, and volume serial | Device firmware, storage stack, file system |
| **MOTHERBOARD** | SMBIOS Type 2 manufacturer, product, version, serial, asset tag, and location, with WMI fallback | Platform firmware |
| **CHASSIS** | SMBIOS Type 3 manufacturer, type, version, chassis serial, and asset tag | Platform firmware |
| **(SM)BIOS** | BIOS vendor/version/date plus SMBIOS system UUID, identifying number, system serial, SKU, and family | Platform firmware |
| **SYSTEM INFORMATION** | Windows Product ID, firmware product key when exposed, MachineGuid, hardware-profile GUID, and install date | Windows and registry state |
| **RAM MODULES** | Slot, manufacturer, part number, capacity, and serial reported through `Win32_PhysicalMemory` | SPD/SMBIOS view |
| **CPU** | Processor name, processor ID, any exposed serial, CPUID vendor, family, model, and stepping | Processor and OS view |
| **TPM MODULES** | State, manufacturer, firmware/specification versions, EK public-key hash, and parsed certificate serial, thumbprint, and issuer when available | Security hardware |
| **USB DEVICES** | USB PnP device name and serial parsed from the device instance | Device firmware and PnP |
| **GPU INFO** | GPU name, PnP/hardware ID, NVIDIA UUID when `nvidia-smi` provides one, and board part number when available | Device firmware and PnP |
| **MONITOR INFORMATION** | EDID manufacturer, model, product code, text and numeric serials, and manufacture week/year | Monitor EDID |
| **NETWORK ADAPTERS (NIC's)** | Product/device/hardware IDs, current MAC, and whether a Windows `NetworkAddress` override is present. When an override exists, the current source displays a warning in the **Permanent MAC** row. It does not read the burned-in address | NIC firmware, driver, registry |
| **BLUETOOTH ADAPTERS** | Adapter name and local radio address when Windows exposes it | Radio and registry state |
| **ARP INFO/CACHE** | IPv4 and IPv6 neighbor entries grouped by interface. The `arp.exe` fallback is IPv4-only | Local network runtime |

Microsoft documents the underlying Windows views for [baseboards](https://learn.microsoft.com/en-us/windows/win32/cimwin32prov/win32-baseboard), [physical memory](https://learn.microsoft.com/en-us/windows/win32/cimwin32prov/win32-physicalmemory), [monitor IDs](https://learn.microsoft.com/en-us/windows/win32/wmicoreprov/wmimonitorid), and [TPM state](https://learn.microsoft.com/en-us/windows/win32/secprov/win32-tpm). **[A]**

> [!NOTE]
> The 14 sections are not a complete inventory of every interface. The current NIC provider filters on WMI's `Ethernet 802.3` adapter type and can omit other adapters. The USB provider uses a device-instance parsing heuristic. The monitor provider's numeric-serial lookup matches only the manufacturer code, so two monitors from the same manufacturer can be misassociated. These limits are confirmed in the current source. **[A]**

Keyboards, mice, headsets, docks, webcams, and adapters can contribute USB identity data. Baseline them like any other peripheral. A device missing from **USB DEVICES** is not proof that it has no serial because the current parser intentionally skips several device-instance patterns. **[A]**

## Persistent and software-level identifiers

Persistence describes where a value lives. It does not describe who can see it or whether it is globally unique.

| Layer | Examples | What a reboot or Windows reinstall normally does |
|---|---|---|
| Component firmware | SSD serial, NIC permanent MAC, USB serial, SPD serial, EDID serial | Usually unchanged. Formatting or reinstalling Windows does not rewrite component firmware. |
| SMBIOS platform data | System UUID, baseboard serial, chassis serial | Usually unchanged by a Windows reinstall. Firmware service, an OEM tool, or hardware replacement can change it. |
| UEFI variable store | `Boot####`, `BootOrder`, security databases, and vendor variables | Firmware and operating-system setup can create or update variables. A changed variable is not automatically a hardware-identity change. See the [NVRAM guide](../nvram-spoofing/nvram-spoofing.md#why-this-guide-does-not-delete-variables). **[A]** |
| TPM protected state | Endorsement seed-derived keys, EK public key, EK certificate | A Windows reinstall does not by itself establish a new endorsement identity. A standard TPM clear resets state but must not be assumed to replace the endorsement seed. **[A]** [TCG TPM 2.0 Architecture](https://trustedcomputinggroup.org/wp-content/uploads/Trusted-Platform-Module-2.0-Library-Part-1-Architecture_Version-185_pub.pdf) |
| Windows registry and setup state | MachineGuid, hardware-profile GUID, install date, current MAC override | Belongs to the Windows installation or driver configuration. Do not assume that reinstalling or editing one value changes any underlying hardware identity. |
| File system and partitioning | Volume serial, GPT disk/partition GUIDs | Formatting or repartitioning can change these without changing the drive's manufacturer serial. |
| Network runtime | ARP entries, addresses learned from a gateway, DHCP state | Changes as interfaces and neighbors change. It is not a permanent hardware inventory. |

A Windows MAC override illustrates the distinction. NDIS can read a software-configurable `NetworkAddress` from the registry, while the adapter still has a permanent address. **[A]** [Microsoft NdisReadNetworkAddress](https://learn.microsoft.com/en-us/windows-hardware/drivers/ddi/ndis/nf-ndis-ndisreadnetworkaddress)

## Plan the work in the right order

Use this order so each measurement has one clear cause:

> [!WARNING]
> This sequence is a project workflow, not a universal vendor procedure. It has not been validated on every platform and is **[S]**. Any firmware or device write keeps the evidence grade and warning from its dedicated guide. Do not use this summary as a write procedure.

1. **Write down the privacy goal.** Define which observer and which identifier groups are in scope. Do not change fields merely because a tool displays them.
2. **Take a complete baseline.** Export all 14 HWIDChecker sections and, if useful, a batch-script snapshot. Store the exports privately.
3. **Prepare recovery.** Back up data, confirm the backup opens, save BitLocker recovery keys, record firmware versions, and read the exact board or device recovery procedure.
4. **Resolve devices that cannot be changed.** Disconnect, disable, or replace them before changing identifiers or installing Windows.
5. **Handle platform firmware first.** Make only supported SMBIOS or TPM changes from an exact hardware guide. Keep NVRAM work read-only unless a platform vendor publishes a model-specific procedure.
6. **Make component-firmware changes next.** Handle storage, NIC, RAM SPD, monitor EDID, and other device-specific work one component at a time.
7. **Set the intended network boundary.** Configure only equipment you own or administer. Do not use ARP poisoning on a shared network.
8. **Power-cycle and verify every persistent change.** A value that returns after reboot was not proven persistent.
9. **Perform the clean Windows installation last.** Windows then enumerates the final hardware state instead of carrying forward the previous installation's device and registry history.
10. **Take a clean after snapshot before restoring accounts or peripherals.** Reconnect one device at a time and rescan if you need to identify what each device contributes.

The ordering itself remains **[S]**. Microsoft separately confirms that Windows Backup can restore apps, settings, files, and Wi-Fi information, which is why this workflow delays restoration until after the clean baseline. **[A]** [Microsoft Windows Backup](https://support.microsoft.com/en-us/windows/experience/backup-recovery/back-up-and-restore-with-windows-backup)

## Device restrictions

First determine whether the identifier is actually fixed. Capture a baseline, disconnect or disable only that device, then rescan. If the field disappears with the device, you have identified its source. **[A]**

Use these rules:

- **External USB device with a fixed serial:** Leave it disconnected during the clean baseline. Use a dedicated, verified vendor procedure only if one exists for the exact controller. Otherwise replace the device if it is inside your threat model.
- **Onboard Ethernet, Wi-Fi, Bluetooth, audio, or graphics:** Prefer a documented UEFI/BIOS disable option. Verify in HWIDChecker that the device is no longer enumerated. A disabled Windows adapter can still remain a known PnP device, so do not equate Device Manager's disabled state with absence from firmware.
- **Removable onboard module:** If firmware cannot disable it, physical removal is an option only when the manufacturer's service manual says the module is removable and the machine is powered down safely.
- **Disk with a fixed controller serial:** Formatting, deleting partitions, changing a volume serial, or placing the disk behind a virtual layer does not prove the physical serial changed. Use the exact [storage-layer guidance](../ssd-spoofing/ssd-spoofing.md#what-a-storage-device-can-expose), or replace the drive.
- **Write-protected SMBIOS:** Stop when the firmware rejects writes. Do not force a cross-model utility or modified BIOS. Use the [motherboard requirements and recovery preparation](../motherboard-spoofing/motherboard-spoofing.md#requirements-and-recovery-preparation) first.
- **TPM tied to the platform:** Do not treat `Clear-Tpm`, a Windows reinstall, or an ordinary BIOS reset as proof of a new endorsement identity. Read [what clearing a TPM changes](../tpm-spoofing/tpm-spoofing.md#what-clearing-the-tpm-changes) and the [fTPM identity-versus-state evidence](../resets/ftpm-reset-tutorial.md#before-you-reset).
- **RAM with fixed or protected SPD:** Use the [DDR4 and DDR5 guidance](../ram-spoofing/ram-spoofing.md#ddr4-and-ddr5-layout) only when the exact SPD device and write protection are understood. Otherwise replace the module if its serial is in scope.
- **Monitor with a fixed EDID serial:** Use the [monitor change-method overview](../monitor-spoofing/monitor-spoofing.md#what-this-changes) for an exact, verified EDID path. Otherwise disconnect or replace the display for the measurement.
- **Gateway or router identity:** Treat it as network infrastructure, not a PC component. Use only your own router and the [routed-isolation options](../arp-spoofing/arp-spoofing.md#options).
- **GPU with no documented persistent method:** This project has no general, verified firmware-write procedure for current GPUs. Do not substitute an unknown kernel driver or runtime hook. Remove or replace the device only if the privacy goal justifies it.

> [!WARNING]
> Firmware menus, removable modules, and write protections are model-specific. The actions above are decision rules, not universal procedures. If the exact board or device manual does not confirm the action, treat it as **[S]** and stop before changing hardware.

## Safety checklist

Every part guide links back here. Apply this list before any firmware, SPD, EDID, TPM, storage, or network write.

- Back up important files and verify the backup from another device.
- Save every BitLocker recovery key before TPM, Secure Boot, boot-order, storage, or firmware work. Firmware and TPM changes can trigger recovery. **[A]** [Microsoft BitLocker recovery overview](https://learn.microsoft.com/en-us/windows/security/operating-system-security/data-protection/bitlocker/recovery-overview)
- If a firmware or TPM change is planned, suspend BitLocker with Microsoft's documented procedure and resume it after the machine boots normally. **[A]** [Microsoft: suspend BitLocker for non-Microsoft firmware updates](https://learn.microsoft.com/en-us/troubleshoot/windows-client/windows-security/suspend-bitlocker-protection-non-microsoft-updates)
- Record original identifiers and firmware versions before writing anything.
- Use only firmware for the exact model and hardware revision. Download a stock recovery image only from the board or device vendor. Boards revision 1.x and 2.x of the same model can need different files. Verify vendor checksums or signatures when published.
- Read the exact BIOS recovery procedure before flashing. A ROM dump is not automatically a usable recovery image.
- Use stable power. Do not interrupt firmware writes.
- Disconnect non-target storage before using any mass-production or erase tool.
- Change one layer at a time. Reboot, rescan, and compare before moving on.
- Stop on an unexpected model, capacity, controller, NAND, firmware, certificate, or write-protection result.
- Do not disable Secure Boot, TPM, BitLocker, virtualization security, or driver-signing protections as a generic first step. Change a security control only when the exact documented procedure requires it, and restore it afterward.
- Keep before/after exports private. They are an identity inventory.

## Take before and after snapshots

### HWIDChecker.exe

1. Run [HWIDChecker.exe](/HWIDChecker.exe). The current application manifest requests administrator rights at launch. **[A]**
2. Wait for all 14 sections to finish. A provider error appears inside that section rather than cancelling the whole scan.
3. Select **Export**. The app writes a timestamped `HWID-EXPORT-*.txt` file beside the executable and shows the full path. Label a private copy `before` with the date, hardware configuration, and firmware versions.
4. Make one approved change, then perform the reboot or full power cycle required by the dedicated guide.
5. Run the same version of HWIDChecker again and export an `after` copy.
6. Compare the relevant fields and the full output. Check that an intended change persisted and that unrelated manufacturer, model, capacity, firmware, or certificate fields did not change unexpectedly.

For firmware-level changes, take three captures: before the change, immediately after the first reboot, and again after a full shutdown and cold boot. A value that returns after a cold boot was not proven persistent.

For storage comparisons, record the connection path (native M.2, direct SATA, USB bridge, or RAID controller) and use the same path for both captures. A transport change can change what Windows is able to query. **[A]**

> [!CAUTION]
> The exports can contain a Windows product key, stable hardware identifiers, TPM certificate data, and network addresses. Keep them private. Redact identifiers before sharing excerpts.

Do not use **Clean Devices**, **Clean Logs**, or the updater as part of measurement. Those paths change system state and are not required for a before/after comparison.

### Batch-script fallback

Use [the Windows 10 script](/app/scripts/hwid-check-w10.bat) or [the Windows 11 script](/app/scripts/hwid-check-w11.bat) when the GUI is unavailable. They display and can export 12 categories: disk, CPU, system, motherboard, BIOS, RAM, TPM, GPU, USB, monitor, NIC, and ARP data. Pressing a key at the prompt writes a timestamped `HWID_CHECK_EXPORT_*.txt` beside the script. **[A]**

The batch scripts are not equivalent to the GUI. They omit the dedicated **CHASSIS** and **BLUETOOTH ADAPTERS** sections, do not collect MachineGuid or the hardware-profile GUID, use legacy WMI/WMIC paths in their export routines, and expose fewer low-level storage and TPM details. Use one method consistently for both snapshots.

## Clean Windows reinstall checklist

A clean install removes personal files, apps, settings, and manufacturer customizations. Microsoft calls it an advanced option. **[A]** [Microsoft clean-install instructions](https://support.microsoft.com/en-us/windows/deployment/install-upgrade/reinstall-windows-with-the-installation-media)

### Before booting the installer

- Verify a separate backup by opening representative files from it.
- Save recovery keys for every encrypted internal and external volume. Microsoft documents how to locate a backed-up BitLocker recovery key. **[A]** [Find your BitLocker recovery key](https://support.microsoft.com/en-us/windows/security/encryption/find-your-bitlocker-recovery-key)
- Record the installed Windows edition. Reinstall the matching edition so activation can be recovered through supported licensing.
- Create current installation media from Microsoft, not a third-party ISO mirror.
- Download the exact chipset, storage, network, and graphics drivers from the PC or component vendor.
- Finish and verify approved firmware/device changes before the reinstall.
- Disconnect backup drives and every drive that is not meant to be erased. Leave only the installer and the internal disks that are explicitly in wipe scope.

### During setup

1. Boot the official installation media using the PC manufacturer's documented boot menu.
2. Choose the clean-install path and confirm that files, apps, and settings will be deleted.
3. At the disk-selection screen, follow Microsoft's clean-install procedure: delete the partitions on **Disk 0** until only **Disk 0 Unallocated Space** remains, then select it. Microsoft explicitly warns not to modify other listed disks. Disconnect non-target disks before setup if the numbering could be ambiguous. **[A]**
4. Treat partition deletion as a logical clean install, not certified data sanitization. Microsoft's `diskpart clean all` writes zeros to every sector, but it is a different, destructive operation and is not required merely to reinstall Windows. **[A]** [Microsoft `clean` command](https://learn.microsoft.com/en-us/windows-server/administration/windows-commands/clean)
5. Follow the supported account and network flow shown by the installer. Current Windows 11 requirements say that Home and Pro for personal use need internet connectivity and a Microsoft account during initial setup. Do not use undocumented bypass scripts. **[A]** [Microsoft Windows 11 requirements](https://www.microsoft.com/en-us/windows/windows-11-specifications)
6. Do not restore a previous PC backup during setup. Microsoft states that signing in with the same account can restore app pins, settings, files, and Wi-Fi information. **[A]**

> [!NOTE]
> Setup screens can change by edition and build. Follow the current Microsoft flow, decline restoration of an old PC, and take the clean baseline before adding other accounts or devices.

### After the first desktop

- Install only official device drivers required for the baseline.
- Apply Windows security updates, then reboot.
- Run HWIDChecker and save the clean after snapshot before restoring personal accounts, Windows Backup, cloud sync, or old peripherals.
- Reconnect one device at a time. Rescan after each connection when you need to attribute a new serial or PnP instance.
- Re-enable security features that were temporarily changed, when the exact hardware guide and recovery state say it is safe. Confirm BitLocker protection is in the intended state.

## Known-spoofable hardware

This table lists one row per part guide. It does not recommend a seller or promise that every retail device with the same name contains the same controller.

> [!WARNING]
> Entries marked **[S]** are research leads, not verified procedures. Do not buy hardware or run a write tool from this table alone. Open the linked guide, verify the exact controller, board revision, firmware, NAND or EEPROM, backup path, and recovery method first.

| Part | What can be changed or substituted | Evidence | Guide |
|---|---|---|---|
| Motherboard | Type 1 system, Type 2 baseboard, and Type 3 chassis fields on an AMI Aptio platform with firmware that permits SMBIOS writes | Field definitions **[A]**; exact write support and switches **[S]** | [Motherboard: what changes](../motherboard-spoofing/motherboard-spoofing.md#what-this-changes) |
| NVRAM / EFI variables | No generic change is supported. Use read-only inventory unless the vendor publishes an exact procedure | Read-only inspection **[A]**; unknown writes **[S]** | [NVRAM: why writes are excluded](../nvram-spoofing/nvram-spoofing.md#why-this-guide-does-not-delete-variables) |
| Storage | Controller-reported model, serial, and firmware on supported controllers: Maxio MAP1202, Silicon Motion SM2263XT (serial and, where the exact firmware supports it, namespace EUI-related fields), and the YANSEN / KingSpec 2.5-inch workflow (model, serial, firmware, optional WWN). A Realtek RTL9210B USB enclosure can change USB/SCSI bridge strings and the bridge serial; that does not rewrite the SSD's native NVMe identity | MAP1202 and YANSEN / KingSpec: project-owner tests **[C]**, not independently repeated. SM2263XT: controller characteristics **[A]**, write workflow **[S]**. USB bridge: layer distinction **[A]**, bridge behavior **[CC]**, write workflow **[S]** | [SSD: M.2 workflow](../ssd-spoofing/ssd-spoofing.md#m2-ssd-spoofing), [SM2263XT notes](../ssd-spoofing/ssd-spoofing.md#silicon-motion-sm2263xt-notes), [USB bridge identity](../ssd-spoofing/ssd-spoofing.md#usb-nvme-enclosures-and-bridge-serials), [SSD: 2.5-inch workflow](../ssd-spoofing/ssd-spoofing.md#normal-25-ssd-spoofing) |
| NIC / MAC | Current software MAC override through NDIS `NetworkAddress` (the permanent MAC remains separate); firmware base MAC on the Mellanox ConnectX-3 CX311A / MCX311A-XCAT; controller EEPROM, NVM, OTP, or eFuse MAC where the dedicated guide matches an exact supported Intel, Realtek, or ASIX controller | Override model **[A]**; ConnectX-3 named hardware test **[C]**; storage models **[A]**; individual procedures range from **[C]** to **[S]** | [MAC: Windows override](../mac-spoofing/mac-spoofing.md#windows-networkaddress-override-software-only), [MAC: ConnectX-3 workflow](../mac-spoofing/mac-spoofing.md#mellanox-connectx-3-cx311a--mcx311a-xcat), [MAC: controller storage](../mac-spoofing/mac-spoofing.md#controller-storage-efuse-eeprom-or-flash) |
| RAM | Module SPD identity fields on a supported DDR4 EE1004 EEPROM or DDR5 SPD5118 hub with a compatible external programmer, subject to hardware and software write protection | Field layout and protection model **[A]**; write workflow **[S]** | [RAM: external-programmer procedure](../ram-spoofing/ram-spoofing.md#external-programmer-procedure) |
| Monitor | EDID manufacturer, model, product code, and serial presented downstream, through a programmable EDID emulator or a display with a documented writable EDID | Observable fields **[A]**; write procedure device-specific | [Monitor: change methods](../monitor-spoofing/monitor-spoofing.md#what-this-changes) |
| TPM | dTPM: replacing a board-compatible discrete module substitutes another TPM and its endorsement identity. This is hardware replacement, not a software spoof. fTPM: a standard clear changes state, not the EPS; some firmware-specific identity-change reports exist, with mixed evidence | dTPM identity model **[A]**, compatibility board-specific; fTPM clear semantics **[A]**, platform results range from **[C]** and **[CC]** to **[S]** | [TPM: implementation types](../tpm-spoofing/tpm-spoofing.md#tpm-implementation-types), [fTPM reset evidence](../resets/ftpm-reset-tutorial.md#before-you-reset) |
| Router or gateway | A routed isolation router under your control, including supported OpenWrt hardware, substitutes the first-hop gateway and LAN-side MAC visible in the PC's neighbor table | Routed design **[A]**; exact menu is device-specific | [ARP: what changes](../arp-spoofing/arp-spoofing.md#what-this-changes) |

AMI publishes Aptio firmware utilities, but public availability of a utility does not prove that an end-user board permits a particular write. **[A]** [AMI Aptio Utilities](https://www.ami.com/resources/aptio-utilities/)

## Reported anti-cheat status

These statuses are community-reported. This project has not verified them. Every row is **[S]**. The date is the date of the last reported issue, not a re-test.

| Game(s) | Anti-cheat | Reported status | Last reported issue | Reported requirements |
|---|---|---|---|---|
| Rust | EasyAntiCheat | Undetected | 2026-08-16 ([RAID 0](../ssd-spoofing/ssd-spoofing.md#raid-disk-identity-and-volume-identity), [NVRAM](../nvram-spoofing/nvram-spoofing.md)) | dTPM, Secure Boot, IOMMU |
| Fortnite | EasyAntiCheat | Undetected | 2026-08-16 ([RAID 0](../ssd-spoofing/ssd-spoofing.md#raid-disk-identity-and-volume-identity), [NVRAM](../nvram-spoofing/nvram-spoofing.md)) | dTPM, Secure Boot, IOMMU |
| Any | BattlEye | Undetected | none listed | none listed |
| Any | EA Javelin | Undetected | none listed | dTPM, Secure Boot |
| Any | Tencent ACE | Undetected | none listed | dTPM, Secure Boot, IOMMU |

The reported requirements are platform settings, not spoofing steps. **dTPM** is a discrete TPM module: a separate chip with its own endorsement identity, unlike a firmware TPM. **Secure Boot** is the UEFI feature that allows only signed boot loaders; the reported setups run with it on. **IOMMU** (Intel VT-d, AMD-Vi) is the processor feature that controls how devices access memory; the reported setups run with it on too. See [TPM implementation types](../tpm-spoofing/tpm-spoofing.md#tpm-implementation-types) for the dTPM-versus-fTPM difference. **[S]**

## Sources

- [DMTF SMBIOS Specification 3.10.0](https://www.dmtf.org/sites/default/files/standards/documents/DSP0134_3.10.0.pdf)
- [Trusted Computing Group EK Credential Profile 2.7](https://trustedcomputinggroup.org/wp-content/uploads/TCG-EK-Credential-Profile-for-TPM-Family-2.0-Level-0-Version-2.7_Pub.pdf)
- [Trusted Computing Group TPM 2.0 Library Part 1, Architecture, version 185](https://trustedcomputinggroup.org/wp-content/uploads/Trusted-Platform-Module-2.0-Library-Part-1-Architecture_Version-185_pub.pdf)
- [Microsoft: Hardware IDs](https://learn.microsoft.com/en-us/windows-hardware/drivers/install/hardware-ids) and [Device Instance IDs](https://learn.microsoft.com/en-us/windows-hardware/drivers/install/device-instance-ids)
- [Microsoft: Win32_BaseBoard](https://learn.microsoft.com/en-us/windows/win32/cimwin32prov/win32-baseboard), [Win32_PhysicalMemory](https://learn.microsoft.com/en-us/windows/win32/cimwin32prov/win32-physicalmemory), [WmiMonitorID](https://learn.microsoft.com/en-us/windows/win32/wmicoreprov/wmimonitorid), and [Win32_Tpm](https://learn.microsoft.com/en-us/windows/win32/secprov/win32-tpm)
- [Microsoft: NdisReadNetworkAddress](https://learn.microsoft.com/en-us/windows-hardware/drivers/ddi/ndis/nf-ndis-ndisreadnetworkaddress) and [GetVolumeInformationW](https://learn.microsoft.com/en-us/windows/win32/api/fileapi/nf-fileapi-getvolumeinformationw)
- [Microsoft: Reinstall Windows with installation media](https://support.microsoft.com/en-us/windows/deployment/install-upgrade/reinstall-windows-with-the-installation-media)
- [Microsoft: Windows 11 specifications and setup requirements](https://www.microsoft.com/en-us/windows/windows-11-specifications)
- [Microsoft: Windows Backup](https://support.microsoft.com/en-us/windows/experience/backup-recovery/back-up-and-restore-with-windows-backup)
- [Microsoft: BitLocker recovery overview](https://learn.microsoft.com/en-us/windows/security/operating-system-security/data-protection/bitlocker/recovery-overview)
- [Microsoft: suspend BitLocker for non-Microsoft firmware updates](https://learn.microsoft.com/en-us/troubleshoot/windows-client/windows-security/suspend-bitlocker-protection-non-microsoft-updates)
- [Microsoft Q&A: choosing a machine identifier](https://learn.microsoft.com/en-us/answers/questions/5762504/unique-id-of-machine) (supporting context, not a Windows product specification)
- [AMI Aptio Utilities](https://www.ami.com/resources/aptio-utilities/)
- [HWIDChecker hardware providers](https://github.com/Fundryi/HWID-Privacy/tree/main/app/src/Hardware) and [Windows batch scripts](https://github.com/Fundryi/HWID-Privacy/tree/main/app/scripts)
