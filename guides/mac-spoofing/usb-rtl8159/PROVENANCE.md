# rtl8159-flasher provenance

This is this project's own tool, not a mirror of a third-party download. It was built by this project for the [MAC guide](../mac-spoofing.md#realtek-rtl8159-10gbe-lekuo-dr59r11-recommended).

- Name: `rtl8159-flasher.exe`
- Version: 1.0.1 (public build, contains no data from any specific adapter; the self-test uses a synthetic image)
- Platform: Windows x64. The executable requests administrator rights.
- Purpose: read, back up, and change the stored MAC and the USB serial of a Realtek RTL8159 USB 10GbE adapter, through the Windows network driver's own control link. Nothing else on the chip is touched.
- Supported: adapters with USB ID `0BDA:815A` and RTL8159 chip ID `0x2020`. Verified on the Lekuo DR59R11 only. The tool lists no other adapters and refuses a chip whose ID does not match.

## Embedded files

None. The executable is a single file with no vendor files and no dependencies. It uses the Windows built-in Realtek driver or the Realtek driver from the Lekuo download.

At run time the tool writes one backup file per adapter to the `backups` folder next to the exe (`backups\<factory MAC>.rtl8159.json`). It holds the first full read, the latest read, and an append-only history of reads and changes.

## Integrity

| File | Bytes | SHA-256 |
|---|---:|---|
| `rtl8159-flasher.exe` | 10,344,960 | `a5165d4db5285d25462296b3a0c4d668104b96d9414d484c4b3fc5f7eb43071e` |
