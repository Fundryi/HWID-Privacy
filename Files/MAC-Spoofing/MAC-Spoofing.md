# MAC Address Spoofing Guide

This guide provides instructions for spoofing MAC addresses on different network interface controllers (NICs).

### Prerequisites

- Windows 10/11 OS
- If you have a Realtek NIC:
  - Download required tools:
    - [RealTekNicPgW2.7.5.0.zip](Realtek%20Files/RealTecNicPgW2.7.5.0.zip)
    - [realtek_efuse_prog.zip](Realtek%20Files/realtek_efuse_prog.zip)
- If you have an Intel NIC:
  - Download required tools:
    - [Intel Files/EEupdate_5.35.12.0.zip](Intel%20Files/EEupdate_5.35.12.0.zip)

---

## Realtek NICs

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

## Intel NICs

For Intel network cards, you can use the EEUPDATE utility through a DOS bootable USB.

### Prerequisites

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

---

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

## Additional Resources

- Always verify the authenticity of tools before using them
- Consider backup plans in case of failed MAC address changes
- Test network connectivity after making changes
