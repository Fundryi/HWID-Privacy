# Hardware Identification (HWID) Privacy Guide

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
      - [✅ fTPM Spoofing](#-ftpm-spoofing)
      - [⚠️ dTPM (Not Recommended)](#️-dtpm-not-recommended)
  - [HWID Checker References](#hwid-checker-references)
  - [Contribution \& Updates](#contribution--updates)
    - [Credits](#credits)

---

## Hardware Categories & Evasion Strategies

### 1. **Motherboard**

#### Serial Spoofing

- **Tool**: **DMIEdit** (for AMI BIOS).
- **Steps**:
  - Change only 2–5 digits of your original serial.
  - Avoid odd patterns (e.g., `SPOOFER-XXXX`).
    - **Example**:
      - Original: `08ZU9T1_NAVX2ZXV4F`
      - Changed: `08ZU9T1_NABX12XZ4A`
    - The prefix is usually part of a broader ID scheme. Make only reasonable adjustments.
  - Reflash the BIOS.
  - Clear CMOS when done.

#### RGB Control/USB Serials

- **Intel**: Disable USB ports used for RGB in your BIOS (some boards have per-port toggles).
- **AMD (AM4/AM5)**: Unplug RGB headers or use boards with hardware toggles (e.g., ASUS ROG, MSI).
  - **Note**: Some RGB controllers (e.g., MSI Mystic Light) create distinct USB serials.

---

### 2. **Storage**

#### NVMe SSDs (M.2)

- **Controller**: Maxio MAP1202 NEEDED
- **How-To**:
  - [M.2 SSD Spoofing](/Files/SSD-Spoofing/SSD-Spoofing.md/#m2-ssd-spoofing)

#### SATA SSDs (2.5")

- **Controller**: YANSEN SSD NEEDED
- **How-To**:
  - [NORMAL 2.5' SSD Spoofing](/Files/SSD-Spoofing/SSD-Spoofing.md/#normal-25-ssd-spoofing)

> Modifying these drives can void warranties.  
> Software/Bios-based RAID0 is generally virtual and unsafe for HWID evasion.

#### Hardware RAID

- **Why**: A proper hardware RAID controller prevents the OS (and anti-cheats) from querying individual drive serials.
- **Examples**:
  - **S322M225R** (for M.2 drives).
  - **LSI/MegaRAID** models for SATA/SAS drives.
- **Note**: True hardware RAID tends to cost more, but it helps mask original drive identifiers on a lower level.

---

### 3. **Network Interface Card (NIC)**

#### MAC Address Spoofing

- **Intel/Realtek NICs**: Permanent changes possible:
  - [Intel NIC MAC Spoofing Guide](Files/MAC-Spoofing/MAC-Spoofing.md#intel-nics)
  - [Realtek NIC MAC Spoofing Guide](Files/MAC-Spoofing/MAC-Spoofing.md#realtek-nics)
  - Note: Certain models might not support flashing or may revert.
- **USB-Based**: Use adapters with **AX88179** chips (steer clear of uncertain “A/B” revisions).
  - **Spoof Utility**: [ASIXFlash Repository](https://github.com/jglim/ASIXFlash).
    - Backup if original repository went down: [ASIXFlash-master.zip](/Files/MAC-Spoofing/USB%20AX88179%20Files/ASIXFlash-master.zip).
- **Purchasable HWID Spoofers**: Some handle NIC spoofing, but certain NICs resist it, and they can produce questionable serial data in other areas.
- **Recommended USB NIC**: [Taobao Link via Sugargoo](https://www.sugargoo.com/#/home/productDetail?productLink=https%253A%252F%252Fitem.taobao.com%252Fitem.htm%253Fid%253D745242613972)
- **Best Practice**: Keep the first 6 digits (vendor ID), change only the last 6.

---

### 4. **GPU**

- **NVIDIA**: UUID accessible via `nvidia-smi`.
  - **No stable public spoofing guide** is widely known. Advanced hooking at the driver level may exist, but it's risky and can be flagged.
    - (Just don't even think about it; get AMD if you cheat, unless you're ready to spend money.)
    - nVidia GPU UUID are NOT unique, but still can be used to track you in some ways.
- **AMD**: No publicly documented UUID. Generally seen as safer for HWID privacy.

---

### 5. **RAM**

- **Null Serials**:
  - Corsair DDR4/DDR5
  - GEIL DDR4/DDR5DDR4/DDR5
  - Trident Z G.Skill DDR4/DDR5

---

### 6. **USB Peripherals**

- **Keyboards/Mice**:
  - Roccat (now Turtleshell), Xtrfy models and Razer DeathAdder V3 should not have USB serials.
- **USB Sticks**: Some “UDisk” drives default to `00000000`
  - Verify with **USBDeview**.
- **Avoid**: Devices with hardcoded hardware serials you cannot edit.
  - Don't fall for any spoofer/software that claims to hide it via REGEDITs; they are USELESS.
  - Any smart AC/software would always get the serials directly from the USB protocol, which they do. Changing the registry to block reading them will NOT hide anything. You can try it yourself: hide your USB devices via regedit, then open a USB debugger; it will still show the serials.

---

### 7. **EDID / Monitor Spoofing**

- **Why It Matters**: Monitors contain EDID data with a potentially unique serial.
- **Tools**:
  - **Fuser** or **Dr.HDMI** ([4K version](https://www.hdfury.eu/shop/drhdmi4k/)).
  - EDID can be dumped, edited in a hex tool, and re-flashed via these devices.
- **Result**: The monitor appears as a different device, reducing traceability.
- Using a Fuser on Faceit is not recommended, even with EDID spoofing.

---

### 8. **Router (ARP Table Isolation)**

- **Hardware**: GL.iNet running OpenWrt firmware or a custom-flashed OpenWrt router.
- **Process**:
  - Change the router's MAC and hostname.
  - Change the MAC of the port you're using on the router.
    - (_This is different from the router's main MAC!_)
  - Plug only your gaming PC into the router's LAN port.
  - Connect the router's WAN port to your home router.
    - Avoid connecting other devices, so the ARP table shows only your gaming PC.
    - And don't worry about those ARP addresses; these are normal and not unique. They're created by Windows:
      - `224.0.0.22 01-00-5e-00-00-16 static`
      - `224.0.0.236 01-00-5e-00-00-ec static`
      - `224.0.0.251 01-00-5e-00-00-fb static`
      - `224.0.0.252 01-00-5e-00-00-fc static`
      - `192.168.8.255 ff-ff-ff-ff-ff-ff static`
      - `239.255.255.250 01-00-5e-7f-ff-fa static`
      - `255.255.255.255 ff-ff-ff-ff-ff-ff static`

---

### 9. **TPM**
- **Warning**: dTPM is flagged by some anti-cheats (e.g., FaceIT).
- **Current Recommendation**: Use **fTPM** for FaceIT/VGK. 
  - Since 04.04.25, VGK enforces **fTPM** if you're flagged, dTPM won't work there anymore.

#### ✅ fTPM Spoofing
- **Concept** (complicated but even might on AMD):
  - [fTPM Spoof PoC by cycript](https://github.com/cycript/FTPM_POC)
- **Simpler Working Method**:
  - **Requirements**:
    - Intel platform
    - Motherboard with:
      - Dedicated USB Flash port
      - BIOS Flash Button
        - Tested: MSI Z790
        - Should work with all Intel boards since the 11th generation release, when the EK went offline.
<details>
  <summary>Intel Forum Confirmation</summary>

  ![Intel Forum](./Files/fTPM/EK%20OFFLINE%20INTEL.png)

</details>
  &#8203;

- **How it works**:
  - Look for your manufacturer’s mobo manual for details
  - Download and put the BIOS file on the stick and plug it into the correct USB slot
    - Each manufacturer has their own way of flashing, many have YouTube videos with examples, please watch them or read their manuals to not mess it up :)
  - Click the Flash Button and let it rewrite mobo sectors
  - This regenerates the fTPM seed
  - Results in a *new, unique fTPM serial* signed by EK
- **Note: Doesn't work on AMD boards**

#### ⚠️ dTPM (Not Recommended)
- Buy a TPM module (e.g., from eBay)
- Plug into your motherboard’s TPM header
- In BIOS:
  - Disable fTPM
  - Enable dTPM

---

## HWID Checker References

- **UNIVERSAL**: [HWIDChecker.exe](/HWIDChecker.exe)
- **Windows 10**: [HWID Checker Script](/HWID-Checkers/Bats/HWID%20CHECK%20W10.bat)
- **Windows 11**: [HWID Checker Script](/HWID-Checkers/Bats/HWID%20CHECK%20W11.bat)

---

## Contribution & Updates

- Additional NIC spoofer models may be listed in the future.
- This guide evolves as new findings emerge.

---

### Credits

> Credits are a weird thing, not everything can be traced, a lot of work/info went into this guide from many many diffrent places and people.
> I will only note the people that I KNOW helped or are resonpsible for these sections in the first place.

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