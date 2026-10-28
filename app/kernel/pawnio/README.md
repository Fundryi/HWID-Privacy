# PawnIO SPD implementation

The code is checked with fake devices and a pin test only. **The embedded setup has not been run, and no live PawnIO device, module initialization or SMBus transaction was performed.** Do not treat a green Rust gate as hardware or lifecycle acceptance.

The existing `Deep hardware information` option requests this source only for a collection containing RAM, with a clear cached gate and fresh gates before install, open, module initialization and each slot. OFF and non-RAM filters do not touch PawnIO. No CLI preparation switch and no runtime download exist.

## Embedded artifacts and session lifecycle

`app/rust/assets/pawnio/` holds the two official artifacts, embedded with `include_bytes!`:

| File | Version | Size | SHA-256 |
|---|---|---|---|
| `PawnIO_setup.exe` | PawnIO.Setup 2.2.0 | 3,410,960 bytes | `1f519a22e47187f70a1379a48ca604981c4fcf694f4e65b734aaa74a9fba3032` |
| `SmbusI801.bin` | PawnIO.Modules 0.2.11 | 55,540 bytes | `a0f7d066e7efda28c0e754c1f52dbd8dc280d388ba8575b514a80cb67490530c` |

The setup is Authenticode-signed by `E=admin@namazso.eu, CN=namazso.eu, O=namazso, L=Debrecen, C=HU`. Upstream ships no standalone driver; the driver is PnP-only and the setup installs a root devnode, so it loads at every boot until uninstalled.

1. If `\Device\PawnIO` already opens, someone else installed PawnIO. The app uses it and never installs or uninstalls it.
2. Otherwise, after a fresh gate check, the app writes the marker `installed-by-hwidchecker` to `%ProgramData%\HWIDChecker\pawnio\`, then writes `PawnIO_setup.exe` there, reopens it read-only with write and delete blocked, checks its SHA-256 and `WinVerifyTrust` (no UI, no online revocation check), and runs `-install -silent` with a 60 s bound. The staged exe is deleted after each run.
3. The app waits up to 10 s for the device, then runs the bounded read.
4. A drop guard runs `-uninstall -silent` (re-staged and re-verified) and deletes the marker on exit code 0. If a request could not be drained, the uninstall is skipped and the marker stays.
5. At every app start, an existing marker triggers the same silent uninstall.

Silent switches were read from the setup's own usage text: `Usage: PawnIOSetup.exe [-install] [-uninstall] [-unrestricted] [-debuginfo] [-silent]`. The setup also registers `"%s\%s" -uninstall -silent` as its own uninstall command. The app never passes `-unrestricted`. The setup source is not public, so exit codes are treated as diagnostics; the device appearing is the install check.

Not implemented: a separate crash-guard process, a `--prepare-kernel-reader` switch, an ACL-protected staging folder (the folder inherits the ProgramData ACL) and persisted quarantine. Licenses: [THIRD-PARTY-NOTICES.md](THIRD-PARTY-NOTICES.md); module source: [modules/](modules/).

## Exact upstream contract and pins

- Driver: PawnIO **2.2.0**, `IOCTL_PIO_VERSION` reply `0x00020200` (four little-endian bytes). Native device name `\Device\PawnIO`, opened through `\\?\GLOBALROOT\Device\PawnIO`; the deprecated DOS link is not required. [Official user-mode header](https://github.com/namazso/PawnIO/blob/master/PawnIO/include/pawnio_um.h), [official library](https://github.com/namazso/PawnIO/blob/master/PawnIOLib/PawnIOLib.cpp).
- IOCTLs: buffered device type 41394; module load `0xA1B22084`, execute `0xA1B22104`, version `0xA1B22184`. Execute input is a 32-byte NUL-padded ASCII function name followed by little-endian 64-bit Pawn cells; output is 64-bit cells. No invented generic capability query: `ioctl_identity` is the selected module's capability/identity check.
- Module: official signed **SmbusI801.bin, release 0.2.11**, exactly **55,540 bytes**, SHA-256 `a0f7d066e7efda28c0e754c1f52dbd8dc280d388ba8575b514a80cb67490530c`. [Pinned source](https://github.com/namazso/PawnIO.Modules/blob/0.2.11/SmbusI801.p), [release](https://github.com/namazso/PawnIO.Modules/releases/tag/0.2.11). Inspected release ZIP SHA-256 `43608cb89bc84247fef1368a139013f7d043e17db6d6c8dfc9b46bf0905a81f4`; the ZIP was inspected in memory, without installing/staging it.
- The module is never written to disk. The app validates the embedded bytes' size/hash in memory, then passes those same bytes to the driver's module-load IOCTL. This initializes **signed Pawn bytecode in the new file-object VM**, not the system driver/service. PawnIO requires it before any execute IOCTL; a module loaded on somebody else's handle cannot be reused. Closing the handle destroys this VM; it does not unload the existing system driver. Redistribution notices are in THIRD-PARTY-NOTICES.md.
- `ioctl_identity`: no input, three output cells: controller four-character type `i801`, I/O base, PCI identifiers (Intel vendor required). `ioctl_smbus_xfer`: input `[7-bit address, direction (read=1/write=0), command, protocol, optional data]`; byte-data reads use protocol 2 and one scalar output cell. There is no slot token or physical-locator mapping in this upstream ABI.
- Acquire `Global\Access_SMBUS.HTP.Method`, matching the documented upstream `\BaseNamedObjects\Access_SMBUS.HTP.Method` mutex, before initializing/using the module. Busy/abandoned mutexes refuse the session. This is cooperative arbitration, not proof of sole driver/service ownership.

## Supported code paths and layout references

Only the pinned **Intel i801** module is implemented. Probe the standard eight SPD addresses `0x50..0x57`, not arbitrary SMBus addresses. DWORD/cell replies are not SPD words; byte-data replies keep native SPD byte order.

- DDR4 EE1004: 512 bytes, declared manufacturing region, revision 1.x. Read page state via SPA0 receive-byte and select pages using send-byte to `0x36/0x37`. Restore the original page on success/failure. Manufacturer 320–321, serial **325–328**, part **329–348** (20 ASCII bytes). CRC-16/CCITT covers bytes 0–125 and 128–253, with little-endian CRCs at 126–127 and 254–255. [JEDEC-layout manufacturer datasheet](https://advdownload.advantech.com/productfile/PIS/AQD-D4U32N32-SBW1/file/AQD-D4U32N32-SBW1_datasheet20230617170733.pdf), [Linux EE1004 reference](https://github.com/torvalds/linux/blob/master/drivers/misc/eeprom/ee1004.c).
- DDR5 SPD5118: verify MR0/MR1 `0x51/0x18`; accept one-byte legacy mode only. MR11 page bits select eight 128-byte pages; memory command bit 7 is set. Never change the addressing mode, write protection, profiles, PMIC or EEPROM data. Restore MR11 on every exit. When `ioctl_write_protection` reports Intel SPD Write Disable (SMBHSTCFG bit 4), the PCH refuses write-flagged transactions to 0x50-0x57 (seen live on a Z790 with DDR5). The page select then uses `ioctl_smbus_xfer` protocol 4 (Process Call) with the read bit, command 0x0B, data = page, as Memtest86+ PR #441 and RAMSPDToolkit 1.2.0 do; the module's Process Call branch passes the caller's direction to SMBHSTADD. Every select is read back from MR11; a mismatch stops the slot before any data read. Source: `docs/research/2026-10-28-ddr5-spd-intel-write-protect.md` (local). 1,024 bytes, revision 1.x, manufacturer 512–513, serial **517–520**, part **521–550** (30 ASCII bytes). CRC bytes 510–511 cover 0–509. [JEDEC-layout manufacturer datasheet](https://advdownload.advantech.com.cn/productfile/PIS/AQD-D5V32GE48-SB/file/AQD-D5V32GE48-SB_datasheet20221005113909.pdf), [Linux SPD5118 reference](https://github.com/torvalds/linux/blob/master/drivers/hwmon/spd5118.c), [hub interface reference](https://docs.buspirate.com/docs/devices/ddr5/).

These are the revision-1 manufacturing layouts described by JESD21-C Annex L and JESD400-5; the complete licensed JEDEC specifications were not obtained in this task. Manufacturer datasheets and upstream implementations were checked. The supported controller/hub/spec revision matrix must be established against actual captures before live acceptance. No general revision-2/I3C/other-controller support is claimed. Manufacturing fields are outside the validated CRC regions. Eight-digit serial output retains ascending SPD byte order; explicit word-swapped captures normalize adjacent bytes first, without heuristics.

## Bounds, output and bring-up still required

The collection caches one snapshot. The coordinator acquires it before providers start, only when RAM is selected; this keeps full/filtered report summaries consistent. Existing table bytes, firmware serial, IDs and source priority remain intact. Independent `SPD Module #n (driver-read, PawnIO)` records show controller/address with an explicit unproven SMBIOS association. Only the manufacturing serial is marked as an ID. Invalid optional fields and duplicate serials retain independent data with value-free diagnostics. Missing dependencies use Phase-0's unavailable message. Gate refusal uses the existing named message. TXT comparison keeps SPD in a separate family and does not assert physical-slot changes from an unpaired serial.

Caller budget: ten seconds plus 500 ms cleanup allowance. Module SMBus waits use its documented 80 ms bound; native pending IOCTL waits are 250 ms, then cancellation/drain is another 250 ms. An undrained request retains its buffers/device/event and quarantines further requests in this process; a synchronous kernel dispatch can outlive the caller worker. Such cases report cleanup incomplete, never successful cleanup, and leave the uninstall to the next app start. A PawnIO installed by someone else is never stopped or uninstalled.

Live bring-up needs an owner-authorized **AC-free lab**: verify the silent install/uninstall, that no `Root\PawnIO` devnode or service is left after uninstall, the driver identity/version and module acceptance; validate i801 board/controller and DDR4/DDR5 hub/page behavior; compare manufacturing bytes with trusted private SPD captures; measure latency/hit rate; exercise concurrent-monitor/busy/arbitration errors, cancellation, page restoration and file-object teardown. Complete the live install/unload/recovery checks before claiming Phase-1 acceptance or preparing a production release. No live check was substituted for this task's mock boundary.
