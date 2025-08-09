# MAC Address Spoofing Guide

This guide provides instructions for spoofing MAC addresses on different network interface controllers (NICs).

### System Requirements

- Windows 10/11

---

## Intel NICs

### Prerequisites

- Download required tools:
  - [EEUPDATE Utility](Intel%20Files/EEupdate_5.35.12.0.zip)

For Intel network cards, you can use the EEUPDATE utility through a DOS bootable USB.

### Setup Steps

1. Create Bootable DOS USB:

   - Download Rufus (https://rufus.ie)
   - Insert your USB drive
   - Select "MS-DOS" as the boot selection
   - Create the bootable drive

2. Prepare Files:
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
  - Do not use this mac, it will brick your network...

3. BIOS Setup:
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

2. Manual Commands (if script fails):

   ```dos
   EEUPDATE /NIC=1 /mac=AABBCCDDEE11
   EEUPDATE /NIC=1 /MAC_DUMP
   ```

3. After Completion:
   - Remove the USB drive
   - Restart the system
   - Boot back into Windows to verify the change
   - Revert your secure boot and CMS settings.

### Important Notes

- Replace `AABBCCDDEE11` with your desired MAC address
- Keep your original MAC address noted down
- The `/NIC=1` parameter targets the first network adapter
  - If you have multiple make sure either to change both or disable the one you dont need/use.
  - `EEUPDATE /LIST_NIC` will list you the NIC's installed.
- Some systems may require specific versions of EEUPDATE
- Not all Intel NICs support MAC address modification
- Incorrect MAC address format can cause network issues

---

## Realtek NICs

### Prerequisites

- Download required tools(trial and error):
  - [RealTekNicPgW2.7.5.0.zip](Realtek%20Files/RealTecNicPgW2.7.5.0.zip)
  - [realtek_efuse_prog.zip](Realtek%20Files/realtek_efuse_prog.zip)

For Realtek network adapters, you can modify the MAC address using the Realtek eFuse Programmer toolkit.

### Programming Steps

1. Modify MAC Address:

   - Open `8168FEF.CFG` file
   - Edit the first line to set your desired MAC address:
     ```
     NODEID = 00 E0 4C 88 00 18
     ;ENDID = 00 E0 4C 68 FF FF
     ```

2. Run the Programming Script:

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

3. Verify MAC Address Change:
   - Open PowerShell
   - Run `ipconfig /all`
   - Look for your network adapter's Physical Address
   - It should match your programmed MAC address

---

## USB NICs

### Realtek USB NICs (Update)
- Status:
  - Realtek-based USB NICs (e.g., RTL8153/RTL8156 series) can also be permanently spoofed.
  - Use the Realtek USB PG Tool package; primary folder to use:
    - “**LATEST_PUB_WIN_USB_PGTOOL_v2.0.22_V2**”
- Tool Package:
  - [RealtekMAC USB.zip](./USB%20Realtek%20Files/RealtekMAC%20USB.zip)
    - Older folders inside are retained only for experimentation; the above folder is the recommended one.
- Tested Hardware:
  - Recommended USB NIC: 
    - [USB‑C 2.5GbE (Uniaccessories)](https://uniaccessories.com/products/usb-c-to-ethernet-adapter-2500mbps)
      - [Amazon DE Link](https://www.amazon.de/-/en/dp/B0C2H9HVH3)
    - Examples that **DONT WORK** at the moment because of missing .CFG settings or custom EFUSE:
      - [UGREEN Product](https://de.ugreen.com/products/ugreen-usb-c-ethernet-adapter-gigabit-lan-adapter-netzwerkadapter-kompatibel-mit-macbook-air-pro-ipad-pro-air-surface-pro-8-7-galaxy-tabs-steam-deck-spielkonsole-switch-und-mehr-typ-c-geraten)
      - [Amazon DE](https://www.amazon.de/dp/B0DNSTHRGQ/)

- Quick Programming Steps (Windows):
  - Open the USB PG Tool from “LATEST_PUB_WIN_USB_PGTOOL_v2.0.22_V2”.
  - Select your device and ensure mode is set to EFUSE.
  - Click “DUMP” to read current settings and confirm the tool returns “PASS”.
![DUMP/Read section](./Images/Realtek%20USB1.png)
  - Set “CURRENT MAC” to your desired value (preserve vendor OUI if possible).
  - Click “PROGRAM” to flash; success should show “PASS”.
![DUMP/Read section](./Images/Realtek%20USB2.png)
  - Done
- Serial Number Note:
  - The tool allows changing the USB “Serial Number”. Avoid changing it in most scenarios:
    - Many Realtek USB NICs share common serial prefixes (e.g., “4013”), so altering it can make your unit uniquely stand out.
  - Do not modify other advanced settings unless you know exactly what they do.

---

### ASIX AX88179(A/B now too!)
- Overview:
  - Permanent MAC changes are possible using the ASIX programming utility.
  - Keep the vendor OUI (first 6 hex digits) and change only the last 6.
- Downloads:
  - [ASIXFlash-master.zip](./USB%20AX88179%20Files/ASIXFlash-master.zip)
    - Upstream reference: [ASIXFlash Repository](https://github.com/jglim/ASIXFlash)
  - [Captain Mac Tool.zip](./USB%20AX88179%20Files/Captain%20Mac%20Tool.zip)
    - Password Used: `captaindma`
      - Not added by me, will also open their website...
- Quick Steps:
  1. Extract the tool, run as Administrator.
  2. Backup current config/EEPROM if the tool provides an option.
  3. Program a new MAC that preserves the original OUI.
  4. Unplug/replug the adapter.
  5. Done
- Notes:
  - AX88179 “A/B” revisions can only be flashed with the Captain Mac Tool.
  - If programming fails or reverts, the unit/firmware may be locked or unsupported.

---