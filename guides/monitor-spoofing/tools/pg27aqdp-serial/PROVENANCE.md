# pg27aqdp-serial provenance

This is this project's own tool, not a mirror of a third-party download. It was built by this project for the [monitor guide](../../monitor-spoofing.md#option-4-firmware-tool-for-the-asus-pg27aqdp).

- Name: `pg27aqdp-serial.exe`
- Version: TBD
- Platform: Windows x64. The executable requests administrator rights.
- Purpose: change or revert the serial an ASUS ROG Swift PG27AQDP with MCM107 firmware stores in its identity record and its EDID serial descriptor, through the monitor's own update interface.
- Supported: ASUS ROG Swift PG27AQDP, firmware MCM107 only. The tool refuses units its checks reject.

## Embedded vendor files

The executable is a single file. It carries these files from the ASUS MCM107 firmware update package, unmodified, so the monitor's update channel can be opened and woken:

- `RhHubAPI.dll` (x86 USB hub interface)
- `FirmwareOneKeyUpdate.exe` and its `.config` file (used only for device detection; the tool never starts an update)
- `ASUSConverter.dll`
- `NvtI2C.dll`
- `Writer_VC_API.dll`
- `config.ini`
- The MCM107 firmware image from the same package (the detection step needs it; the tool never flashes it)

At run time these files are extracted to `%LOCALAPPDATA%\pg27aqdp-serial\`. The same folder holds the user's own original records (`originals`), the plan file and the session log.

## Integrity

| File | Bytes | SHA-256 |
|---|---:|---|
| `pg27aqdp-serial.exe` | TBD | `TBD` |
