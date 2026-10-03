# Hardware Identification (HWID) Privacy Guide

> **Disclaimer:** This guide is for privacy research, hardware fingerprint testing, and education. It is not for violating any Terms of Service.
> **What this guide is:** A deep, technical hardware privacy guide about spoofing/changing hardware identifiers to make a machine as unidentifiable and untraceable as possible under advanced fingerprinting.

> **Read it as a website:** [hwid.idkzal.cc](https://hwid.idkzal.cc)
> **New here?** Start with [Getting Started](guides/getting-started/getting-started.md): what an HWID is, the order of work, and how to check before and after.
> **Anti-cheats:** see the [Reported anti-cheat status](guides/getting-started/getting-started.md#reported-anti-cheat-status) (evidence matrix: vendor statements, artifact-backed reports, and unverified claims, graded).

---

## Table of Contents

- [Hardware Identification (HWID) Privacy Guide](#hardware-identification-hwid-privacy-guide)
  - [Table of Contents](#table-of-contents)
  - [Hardware Categories \& Evasion Strategies](#hardware-categories--evasion-strategies)
    - [1. **Motherboard**](#1-motherboard)
      - [Serial Spoofing](#serial-spoofing)
      - [RGB Control/USB Serials](#rgb-controlusb-serials)
    - [2. **Storage**](#2-storage)
      - [NVMe SSDs (M.2)](#nvme-ssds-m2)
      - [SATA SSDs (2.5")](#sata-ssds-25)
      - [Hardware RAID](#hardware-raid)
    - [3. **Network Interface Card (NIC)**](#3-network-interface-card-nic)
      - [MAC Address Spoofing](#mac-address-spoofing)
    - [4. **GPU**](#4-gpu)
    - [5. **RAM**](#5-ram)
    - [6. **USB Peripherals**](#6-usb-peripherals)
    - [7. **EDID / Monitor Spoofing**](#7-edid--monitor-spoofing)
    - [8. **Router (ARP Table Isolation)**](#8-router-arp-table-isolation)
    - [9. **TPM**](#9-tpm)
  - [HWID Checker References](#hwid-checker-references)
  - [Contribution \& Updates](#contribution--updates)
    - [Credits](#credits)

---

## Hardware Categories & Evasion Strategies

### 1. **Motherboard**

Changes the SMBIOS serials and UUID that Windows, fingerprinting tools, and anti-cheats read from the board.

#### Serial Spoofing

- **Complete Guide**: [Motherboard Spoofing Guide](guides/motherboard-spoofing/motherboard-spoofing.md)
- **NVRAM / EFI variables**: [NVRAM Guide](guides/nvram-spoofing/nvram-spoofing.md)
- **Key Points**:
  - Use **DMIEdit** (for AMI BIOS)
  - Change only 2–5 digits of original serial
  - Avoid odd patterns (e.g., `SPOOFER-XXXX`)
  - Reflash BIOS and clear CMOS after changes

#### RGB Control/USB Serials

- **Intel**: Disable USB ports used for RGB in BIOS (some boards provide per-port toggles).
- **AMD (AM4/AM5)**: Unplug RGB headers or use boards with hardware toggles (e.g., ASUS ROG, MSI).
  - **Note**: Some RGB controllers (e.g., MSI Mystic Light) expose distinct USB serials.

---

### 2. **Storage**

Rewrites the controller-reported model, serial, and firmware of supported SSDs. Volume serials and RAID identities are separate layers that do not prove the drive serial changed.

- **Complete Guide**: [Storage Guide](guides/ssd-spoofing/ssd-spoofing.md) (identifier layers, controller check, USB enclosures, RAID vs volume serials)

#### NVMe SSDs (M.2)

- **Controller**: Maxio MAP1202 needed
- **How-To**:
  - [M.2 SSD Spoofing](guides/ssd-spoofing/ssd-spoofing.md#m2-ssd-spoofing)

#### SATA SSDs (2.5")

- **Controller**: YANSEN SSD needed
- **How-To**:
  - [NORMAL 2.5' SSD Spoofing](guides/ssd-spoofing/ssd-spoofing.md#normal-25-ssd-spoofing)

> Modifying these drives can void warranties.  
> RAID behavior depends on the controller. The logical array identity, and whether member-drive serials pass through, depend on the exact controller, firmware, and driver. Neither hardware RAID nor software/BIOS RAID is established as an accepted identity-change method.

<details><summary>Older info (outdated)</summary>

> Software/BIOS-based RAID0 is generally virtual and unsafe for HWID evasion.

</details>

#### Hardware RAID

- **Why**: A hardware RAID controller hides member-drive serials from the OS only if the controller and its driver do not pass them through. Whether a given anti-cheat still reads member serials through an array is reported both ways; the reports conflict and none includes a versioned test. Controller-level spoofing (above) changes the serial itself and does not depend on this. See [Reported anti-cheat status](guides/getting-started/getting-started.md#reported-anti-cheat-status) and [RAID, disk identity, and volume identity](guides/ssd-spoofing/ssd-spoofing.md#raid-disk-identity-and-volume-identity) in the storage guide.
- **Examples**:
  - **S322M225R** (for M.2 drives).
  - **LSI/MegaRAID** models for SATA/SAS drives.
- **Note**: True hardware RAID tends to cost more. Whether it masks member-drive identifiers depends on the controller, its firmware, and its driver. Check pass-through on your exact controller before you rely on it.

<details><summary>Older info (outdated)</summary>

- **Note**: True hardware RAID tends to cost more, but it helps mask original drive identifiers on a lower level.
- **Why**: A proper hardware RAID controller prevents the OS (and fingerprinting agents) from querying individual drive serials.

</details>

---

### 3. **Network Interface Card (NIC)**

Changes the MAC address the network sees. A Windows `NetworkAddress` override is software-only; a controller write (EEPROM, NVM, OTP, or eFuse) is permanent.

#### MAC Address Spoofing

- **Complete Guide**: [MAC Spoofing Guide](guides/mac-spoofing/mac-spoofing.md)
- **Internal NICs**: Permanent changes possible for Intel, Realtek, and Mellanox:
  - Intel i225 from NVM 1.53 and all i226 versions lock the NVM after a unique MAC is provisioned and the card is power-cycled. Retail i226 is normally already provisioned once, so it is not repeatably rewritable. **[A]** ([Intel Community](https://community.intel.com/t5/Embedded-Connectivity/i226-v-issue-with-eeupdate/td-p/1502910))
  - [Intel NIC MAC Spoofing Guide](guides/mac-spoofing/mac-spoofing.md#intel-nics)
  - [Realtek NIC MAC Spoofing Guide](guides/mac-spoofing/mac-spoofing.md#realtek-nics)
  - [Mellanox ConnectX-3 MAC Spoofing Guide](guides/mac-spoofing/mac-spoofing.md#mellanox-connectx-3-cx311a--mcx311a-xcat) - firmware-level, repeatable, 10 Gbps SFP+
  - Note: Certain models might not support flashing or may revert.
- **USB NICs**: Both Realtek and ASIX adapters support MAC changes:
  - [Complete USB NIC Guide](guides/mac-spoofing/mac-spoofing.md#usb-nics)
  - **Recommended**: [USB‑C 2.5GbE Adapter](https://uniaccessories.com/products/usb-c-to-ethernet-adapter-2500mbps) • [Amazon DE](https://www.amazon.de/-/en/dp/B0C2H9HVH3)
- **Purchasable HWID Spoofers**: Some handle NIC spoofing, but certain NICs resist it, and they can produce questionable serial data in other areas.
- **Best Practice**: Keep the first 6 digits (vendor ID), change only the last 6.

---

### 4. **GPU**

No verified persistent-change method for current cards. Both vendors document a per-unit GPU ID. Which ID a program can read depends on the card, the driver, and the OS.

- **NVIDIA**: UUID accessible via `nvidia-smi`.
  - **No stable public spoofing guide** is widely known. Advanced driver-level hooking may exist, but it's risky and can be flagged.
  - NVIDIA defines the GPU UUID as globally unique and immutable. It also exposes a 64-bit per-device ID (PDI) and, on supported products, a board serial. **[A]** ([nvidia-smi documentation](https://docs.nvidia.com/deploy/nvidia-smi/index.html))
- **AMD**: The Linux AMDGPU driver exposes a persistent `unique_id` on supported GFX9+ GPUs. AMD SMI exposes `amdsmi_get_gpu_device_uuid()` and an ASIC serial, and ROCm SMI has `--showuniqueid`. Support depends on the device and platform; unsupported cards return N/A. **[A]** ([AMDGPU docs](https://docs.kernel.org/gpu/amdgpu/driver-misc.html), [AMD SMI header](https://github.com/ROCm/amdsmi/blob/amd-mainline/include/amd_smi/amdsmi.h))

<details><summary>Older info (outdated)</summary>

- AMD has no publicly documented UUID.
- NVIDIA GPU UUIDs are not always globally unique, but they can still be used for correlation.
- **AMD**: No publicly documented UUID. Generally seen as safer for HWID privacy.

</details>

---

### 5. **RAM**

RAM modules carry a serial in SPD storage. Modules that ship with null serials need no write; otherwise an external programmer can edit the SPD identity fields.

- **Complete Guide**: [RAM Guide](guides/ram-spoofing/ram-spoofing.md)
- **Null Serials**:
  - Corsair DDR4/DDR5
  - GEIL DDR4/DDR5
  - Trident Z G.Skill DDR4/DDR5

---

### 6. **USB Peripherals**

Keyboards, mice, and sticks expose USB serials that fingerprinting stacks can read directly from the protocol. Registry edits do not hide them.

- **Keyboards/Mice**: Serial behavior is per model and per connection mode, not per brand. Check the exact VID:PID.
  - Razer DeathAdder V3 wired (`1532:00b2`): `SerialNumber=0`, so no serial descriptor. One log. **[S]** ([log](https://lists.debian.org/debian-kernel/2026/04/msg00244.html))
  - Razer DeathAdder V3 Pro wired (`1532:00b7`): serial string `000000000000`. The descriptor exists but is zero-filled and not unique. One log. **[S]** ([log](https://paste.cachyos.org/p/7c49eed.log))
  - Razer HyperPolling receiver (`1532:00c3`): `SerialNumber=0`. One log. **[S]** ([OpenRazer issue](https://github.com/openrazer/openrazer/issues/2547))
  - The printed warranty serial on the product is a separate ID. It does not tell you whether a USB serial descriptor exists. **[A]** ([Razer support](https://mysupport.razer.com/app/answers/detail/a_id/548/))
  - No current per-model evidence exists for ROCCAT, whose product lines moved to Turtle Beach, or for Xtrfy.
- **USB Sticks**: Some “UDisk” drives default to `00000000`. This is an unverified historical report. **[S]**
  - Verify with **USBDeview**.
- **Avoid**: Devices with hardcoded hardware serials you cannot edit.
  - Don't trust software claiming to hide USB serials via registry edits. Windows `IgnoreHWSerNum` only changes how Windows builds the PnP instance ID. It does not change the device's descriptor serial, which a program can still request at the protocol level. **[A]** ([Microsoft](https://learn.microsoft.com/en-us/windows-hardware/drivers/usbcon/usb-device-specific-registry-settings))
  - Any advanced fingerprinting stack can pull serials directly from the USB protocol. Registry changes do not hide that data. You can validate this yourself: hide USB devices in the registry, then inspect traffic with a USB debugger; the serials still appear.

<details><summary>Older info (outdated)</summary>

- Roccat (now Turtleshell), Xtrfy models and "all" Razer products should not have USB serials.
- Don't trust software claiming to hide USB serials via registry edits; those methods are useless.

</details>

---

### 7. **EDID / Monitor Spoofing**

Changes the EDID identity (manufacturer, model, serial) a monitor reports to the system, through a software override or an inline emulator.

- **Complete Guide**: [Monitor / EDID Guide](guides/monitor-spoofing/monitor-spoofing.md)
- **Why It Matters**: Monitors contain EDID data with a potentially unique serial.
- **Tools**:
  - **Fuser** or **Dr.HDMI** ([4K version](https://hdfury.com/product/dr-hdmi-4k/)).
  - EDID can be dumped, edited in a hex tool, and re-flashed via these devices.
- **Result**: The monitor appears as a different device, reducing traceability.
- Using a Fuser on 🍊 is not recommended, even with EDID spoofing.

---

### 8. **Router (ARP Table Isolation)**

Puts a router you control between the PC and the primary network, so the PC's ARP table shows the isolation router's MAC as gateway instead of your home router's.

- **Complete Guide**: [Router / ARP Guide](guides/arp-spoofing/arp-spoofing.md)
- **Hardware**: GL.iNet running OpenWrt firmware or a custom-flashed OpenWrt router.
- **Process**:
  - Change the router's MAC and hostname.
  - Change the MAC of the port you're using on the router.
    - (_This is different from the router's main MAC!_)
  - Plug only your target test machine into the router's LAN port.
  - Connect the router's WAN port to your home router.
    - Keep other devices off the isolation router's LAN. The isolated ARP table still shows the gateway router, plus the normal Windows multicast entries below.
    - And don't worry about those ARP addresses; they are normal and not unique. They're created by Windows:
      - `224.0.0.22 01-00-5e-00-00-16 static`
      - `224.0.0.236 01-00-5e-00-00-ec static`
      - `224.0.0.251 01-00-5e-00-00-fb static`
      - `224.0.0.252 01-00-5e-00-00-fc static`
      - `192.168.8.255 ff-ff-ff-ff-ff-ff static`
      - `239.255.255.250 01-00-5e-7f-ff-fa static`
      - `255.255.255.255 ff-ff-ff-ff-ff-ff static`

<details><summary>Older info (outdated)</summary>

  - Avoid connecting other devices, so the ARP table shows only your target test machine.

</details>

---

### 9. **TPM**

The TPM carries its own endorsement identity (EK, EK certificate). Clearing the TPM does not replace it; see the TPM and fTPM reset guides for what does.

- **Complete Guide**: [TPM Spoofing Guide](guides/tpm-spoofing/tpm-spoofing.md)
- **fTPM identity reset (AMD AM5)**: [fTPM Reset Guide](guides/resets/ftpm-reset-tutorial.md)
- **Warning**: dTPM support is product-specific. A faulty dTPM can fail attestation; FACEIT documents this and suggests fTPM as the fix. **[A]** Call of Duty's TPM requirements explicitly list systems with a discrete TPM chip as supported. **[A]** Blanket "dTPM is flagged" rules (e.g., 🍊) are community reports, not vendor statements. **[S]**
- **Current Recommendation**: Use **fTPM** for 🍊/🍒.

<details><summary>Older info (outdated)</summary>

- **Warning**: dTPM is flagged by some strict telemetry stacks (e.g., 🍊).
  - Since 2025-04-04, 🍒 enforces **fTPM** if you’re flagged; dTPM no longer works there.

</details>

---

## HWID Checker References

- **UNIVERSAL**: [HWIDChecker.exe](/HWIDChecker.exe)
- **Windows 10**: [HWID Checker Script](/app/scripts/hwid-check-w10.bat)
- **Windows 11**: [HWID Checker Script](/app/scripts/hwid-check-w11.bat)
- **How to run, export, and compare before/after**: [Take before and after snapshots](guides/getting-started/getting-started.md#take-before-and-after-snapshots)

---

## Contribution & Updates

- Additional NIC spoofer models may be listed in the future.
- This guide evolves as new findings emerge.

---

### Credits

> Credits are a weird thing: not everything can be traced, and a lot of work/info in this guide came from many different places and people.
> I only list the people I know helped or are responsible for these sections in the first place.

- Storage guide/info: [Priventive.de](https://priventive.de/)
- Network guide/info: `fA`, and "`the collective hive mind of the internet`"
- EDID guide/info: `fA` (he did not invent the wheel, only gave me the info)
- The broad base guide/structure was written by a "`French`", [Old Guide Link](https://docs.google.com/document/u/0/d/e/2PACX-1vSjtQF1bSUxN57NXsYKS7haiPvYD68UXg77qinZ4ctcwx7073p9Jbp4W55LdP7vMgmjhZ12DsNHYwft/pub?pli=1)
- `Fundryi` for pasting/collecting this info and putting it out for everyone in one place (☭ ͜ʖ ☭)

meow

⠀⠀⠀⠀⠀⠀⣀⣀⣤⣤⣴⣦⣦⣀⣀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀
⠀⠀⠀⣠⣾⡿⠛⠛⡉⣉⣉⡀⠀⢤⡉⢳⣄⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀
⣀⣤⣾⡿⢋⣴⣖⡟⠛⢻⣿⣿⣽⣆⠙⢦⡙⣧⡀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⢀⣠⠖⠒⠻⠻⠶⣦⣄⠀
⠉⣿⢸⠁⣸⣥⣹⠧⡠⣾⣿⣿⣿⣿⣧⡈⢷⣼⣧⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⣠⣿⣾⣿⣿⣶⣦⣈⠈⠻⣤
⠀⢹⡇⢸⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⠀⣿⣇⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⢸⡟⣫⠀⠙⣿⣿⣿⣿⣷⡄⣽
⠀⠀⠳⣾⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣟⣀⣿⣟⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠸⣷⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿
⠀⠀⠀⠀⠈⠙⠻⢿⣿⣿⣿⣿⣿⣿⣿⠭⠛⠉⠉⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠸⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿
⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠉⠉⠉⠉⠁⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⡠⠒⡊⠙⠉⠉⠉⠓⠲⠤⡀⠀⠀⠙⠿⣿⣿⣿⣿⣿⣿⣿⠏
⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠐⠿⣦⣴⣿⣶⣶⠀⠠⠀⣰⣤⣿⣦⠀⠀⠀⠀⠉⠙⠛⠻⠛⠁⠀
⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠙⢛⠃⢀⣄⠀⣿⡿⠟⠉⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀
⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠊⠟⠋⠈⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀
⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀
⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀
⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀
⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⢦⣄⡀⠀⠀⢀⣀⣠⣴⡿⣿⣦⣀⠀⠀⠀⠀⢀⣴⠏⠀⠀⠀⠀⠀⠀⠀
⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠙⠛⠻⠿⠉⠉⠉⠉⠉⠑⠋⠙⠻⡢⢖⡯⠉⠀⠀⠀⠀⠀
