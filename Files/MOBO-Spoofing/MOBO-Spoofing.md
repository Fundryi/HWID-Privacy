# **MOBO SPOOFING GUIDE**
 
## **Prerequisites**
 
- Recommended way:
  - DMI EDIT WIN64 FILES:
    - [dmi-edit-win64-ami.zip](./dmi-edit-win64-ami.zip)
- Optional:
  - [DMIEDIT GUI v5.27.05.0016 (latest).zip](<./DMIEDIT GUI v5.27.05.0016 (latest).zip>)
  - [dmmiedit GUI (not working on new mobos).zip](<./dmmiedit GUI (not working on new mobos).zip>)
    - This version does not work properly on newer mobos or in general, I'd suggest using the one above!
  - **[HWIDChecker.exe](/HWIDChecker.exe)**
    - (Optional but recommended checking your before/after SSD details)

---

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
