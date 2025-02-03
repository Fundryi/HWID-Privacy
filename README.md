# Comprehensive Hardware Identification (HWID) Evasion Guide

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

- **Controller**: Maxio MAP1202 (allows direct serial/model editing).
- **Steps**:
  1. Use **MXMPTool_MAP1202** with an M.2-to-USB adapter.
  2. Enter realistic serial, firmware, and model names.

#### SATA SSDs (2.5")

- **Option**: YANSEN SSDs with custom flashing software (LINK HERE).
- **Adapter**: A SATA-to-USB with **JMicron JMS578** is recommended for flashing.

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
  - **Spoof Utility**: [ASIXFlash](https://github.com/jglim/ASIXFlash).
- **Purchasable HWID Spoofers**: Some handle NIC spoofing, but certain NICs resist it, and they can produce questionable serial data in other areas.
- **Recommended USB NIC**: [Taobao Link via Sugargoo](https://www.sugargoo.com/#/home/productDetail?productLink=https%253A%252F%252Fitem.taobao.com%252Fitem.htm%253Fid%253D745242613972)
- **Best Practice**: Keep the first 6 digits (vendor ID), change only the last 6.

---

### 4. **GPU**

- **NVIDIA**: UUID accessible via `nvidia-smi`.
  - **No stable public spoofing guide** is widely known. Advanced hooking at the driver level may exist, but it’s risky and can be flagged.
    - (Just don't even think about it; get AMD if you cheat, unless you’re ready to spend money.)
- **AMD**: No publicly documented UUID. Generally seen as safer for HWID privacy.

---

### 5. **RAM**

- **Null Serials**:
  - Corsair/GEIL modules default to `00000000`.
- **Check Tools**: [**HWID CHECKER**](https://github.com/Fundryi/HWID-Privacy/edit/main/README.md#hwid-checker-references)

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

---

### 8. **Router (ARP Table Isolation)**

- **Hardware**: GL.iNet running OpenWrt firmware or a custom-flashed OpenWrt router.
- **Process**:
  - Change the router’s MAC and hostname.
  - Change the MAC of the port you’re using on the router.
    - (This is different from the router’s main MAC!)
  - Plug only your gaming PC into the router’s LAN port.
  - Connect the router’s WAN port to your home router.
    - Avoid connecting other devices, so the ARP table shows only your gaming PC.
    - And don’t worry about those ARP addresses; these are normal and not unique. They’re created by Windows:
      - 192.168.8.255 ff-ff-ff-ff-ff-ff static
      - 224.0.0.22 01-00-5e-00-00-16 static
      - 224.0.0.236 01-00-5e-00-00-ec static
      - 224.0.0.251 01-00-5e-00-00-fb static
      - 224.0.0.252 01-00-5e-00-00-fc static
      - 239.255.255.250 01-00-5e-7f-ff-fa static
      - 255.255.255.255 ff-ff-ff-ff-ff-ff static

---

### 9. **TPM**

- **Warning**: dTPM is flagged on certain anti-cheats (Faceit, Vanguard).
- **Current Recommendation**: Use fTPM for Faceit/VGK if available.
- **dTPM Steps** (if you still try it):
  - Buy a TPM module (online/eBay).
  - Attach it to your motherboard’s TPM header.
  - Disable onboard fTPM in BIOS, enable the discrete module.
  - Reinstall Windows.

---

## HWID Checker References

- **Windows 10**: [HWID Checker Script](https://gist.github.com/Fundryi/2aaebe81aba039756e807cadfe953f91)
- **Windows 11**: [HWID Checker Script](https://gist.github.com/Fundryi/a269ace22cea6d990be8219cbaccda9f)

---

## Contribution & Updates

- Additional NIC spoofer models may be listed in the future.
- This guide evolves as new findings emerge.
