# Third-party notices: PawnIO

`HWIDChecker.exe` embeds two unmodified official PawnIO artifacts. They are used only when the user turns on `Deep hardware information`, RAM is collected and the anti-cheat gate is clear.

## PawnIO setup (driver installer)

- File: `app/rust/assets/pawnio/PawnIO_setup.exe`, embedded unmodified.
- Version: PawnIO 2.2.0, from the GitHub release [namazso/PawnIO.Setup tag 2.2.0](https://github.com/namazso/PawnIO.Setup/releases/tag/2.2.0).
- Size: 3,410,960 bytes.
- SHA-256: `1f519a22e47187f70a1379a48ca604981c4fcf694f4e65b734aaa74a9fba3032`
- Authenticode signer subject: `E=admin@namazso.eu, CN=namazso.eu, O=namazso, L=Debrecen, C=HU`
- Copyright: namazso.
- License: GNU General Public License version 2 or later, with the PawnIO exception stated in the upstream source.
- Corresponding source: [namazso/PawnIO](https://github.com/namazso/PawnIO) (driver, library and license text) and [namazso/PawnIO.Setup](https://github.com/namazso/PawnIO.Setup).

HWIDChecker talks to the driver only through its documented device IOCTLs. It does not link PawnIOLib or copy upstream client code.

## SmbusI801 module

- File: `app/rust/assets/pawnio/SmbusI801.bin`, embedded unmodified.
- Version: PawnIO.Modules 0.2.11, from the GitHub release [namazso/PawnIO.Modules tag 0.2.11](https://github.com/namazso/PawnIO.Modules/releases/tag/0.2.11) (`release_0_2_11.zip`, SHA-256 `43608cb89bc84247fef1368a139013f7d043e17db6d6c8dfc9b46bf0905a81f4`).
- Size: 55,540 bytes.
- SHA-256: `a0f7d066e7efda28c0e754c1f52dbd8dc280d388ba8575b514a80cb67490530c`
- Copyright: (C) 2025 Steve-Tech, and the PawnIO.Modules contributors.
- License: GNU Lesser General Public License version 2.1 or later.
- Corresponding source: `modules/SmbusI801.p`, `modules/sleepmode.inc` and `modules/include/*.inc`, copied unmodified from tag 0.2.11. License text: `modules/COPYING` (from the release ZIP).
