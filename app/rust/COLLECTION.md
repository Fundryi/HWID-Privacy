# HWIDChecker collection reference

Read this file first when changing collection or planning a new identifier. Every collection change must update this reference in the same commit. It describes the current Rust providers, displayed fields, sources, masking and limits; examples are fabricated, never copied from a machine capture.

## Global behavior

- **Administrator only.** The manifest requires elevation; the entrypoint also rejects a non-admin process before collection. A non-admin launch may start, but collection need not work; do not add non-admin fallbacks or change handles for that purpose. Each section below inherits this requirement. Prefer firmware tables, device protocol commands and descriptors when they preserve the existing information.
- **15 parallel sections.** [Provider order and `Ctx`](src/hw/mod.rs), [collection](src/hw/collection.rs): all selected workers start before waiting, each with a 60-second deadline from its own start. Results return in app order. Timeout replaces that section with a timeout body; late results cannot replace it. Workers are detached, so abandoning a wait does not cancel an in-flight OS call.
- **Shared snapshots, per collection.** `Ctx` lazily caches the parsed RSMB SMBIOS table, a present-device instance-ID → first SetupAPI hardware-ID map, and a separate present-instance-ID set. Successes and failures are cached. USB and Bluetooth also perform their own SetupAPI scans. WMI connections are cached per thread and namespace, not shared across workers.
- **Values are source reports.** Firmware strings, driver replies and registry data do not prove uniqueness or authenticity. A Windows disk serial, controller serial, namespace ID, filesystem serial and partition GUID identify different layers. Preserve differing values with distinct labels.
- **Identifier means marked, not necessarily unique.** The tables' **ID** column means the provider records the value in `Section.ids`. `Out::id`, `id_value`, or `combined` with `true` supplies that marking. Unmarked names, model/part codes and status text remain visible.
- **Mask IDs.** [Report formatting](src/report.rs) masks marked values of at least four characters, replacing ASCII letters/digits with `X` and preserving punctuation and other characters. Matching is case-sensitive at whole-token boundaries, longest values first. The raw collection remains intact; views, Copy, Old View and GUI exports use prepared masked copies. This is not complete anonymization: short IDs, unmarked names and error text can remain.
- **Exports.** GUI Export writes UTF-8 without BOM, CRLF `.txt` and `.json` files next to the exe: `HWID-EXPORT-DD.MM.YYYY-HH;MM;SS.txt/.json`, or `...-MASKED.txt/.json` when masking is on. Same-second names overwrite. TXT uses `===== SECTION =====` headings; JSON has `app`, `version`, local `exported` timestamp (no time-zone offset), `masked`, and ordered `sections` containing `title`, `lines`, `ids`. JSON excludes sources, timings and failures. Export during loading can contain `Loading...` bodies.
- **Dump differs from GUI export.** `--dump <file>` writes the full unmasked report with the comprehensive header and centered section headings (93 `=` characters); it also writes `<file>.diag.txt`. Normal item separators are 40 dashes; disk separators are 50. Labels use `Label: value`, combined fields use ` | `, and table widths count UTF-16 units.

### Errors and diagnostics

`E` below means the actual formatted error, not a literal output value. AD numbers refer to the local port approval ledger; behavior is documented here so that ledger is not required to understand collection.

| Convention | Current shape and meaning |
|---|---|
| AD-01 | `operation failed: 0x12345678 detail` (8 uppercase hex digits); preserve the surrounding provider text. Process helpers can include `Process timed out after {ms}ms: {exe} {args}`, `Failed to start process: {exe}`, or `Process exited with code {n}.` Caught panics retain their message inside the collection error. |
| AD-02 | `Error retrieving {SECTION} information: timed out`; provider deadline is 60 s. |
| AD-03 | An unresolved source/item failure becomes visible; a successful fallback normally leaves earlier failures in diagnostics only. Section-specific shapes are listed below. Expected unsupported/absent optional data can also stay diagnostic-only. |
| AD-46 | `{Label}: Unavailable ({E})` keeps other items intact. This is a convention, not the renderer for every failure: BIOS, USB, ARP and other providers retain their specific forms. |
| Collector error | `Error retrieving {SECTION} information: {E}` appended after any existing partial output. A timeout instead supplies a new timeout section. |
| AD-45 | The collector retains lines written before a provider error, rather than losing the section's partial data. |
| `.diag.txt` | Each section: title, `Time: {n} ms`, `Source: {source}`, then `Failed fallback: {source}: {E}` lines. A final `[helpers]` block contains recorded helper errors, including per-device failures. Source is a section-level string, not structured per-field provenance. |

Diagnostics and dumps can contain identifiers in values, paths or errors. Keep real captures private; never paste their identifiers into this public file. A blank provider body displays as `No data available` in the section view/TXT export; it is not proof of hardware absence.

Fenced examples show fabricated section bodies. Optional fields illustrate their formats, not guaranteed availability on one device; trailing table padding is omitted.

### Timing reference (milliseconds)

These are order-of-magnitude observations, not guarantees or a new benchmark. The latest relevant entries in the local `PLANNING-LOG.md` (2026-10-04) take precedence. Where that log has no current figure, the older direct-source research provides context only; it used serial, often warm, non-admin probes before the later additions. Parallel wall time is not the sum of section times, and shared snapshot initialization can charge time to whichever worker reaches it first.

| Section | Latest planning-log figure | Older context / current uncertainty |
|---|---|---|
| DISK DRIVES | No current figure | ~43 ms before NVMe additions; current timing unverified. |
| MOTHERBOARD | No current figure | ~0.03 ms direct firmware; shared snapshot cost varies. |
| CHASSIS | No current figure | ~0.03 ms before SKU addition; current timing unverified. |
| (SM)BIOS | No current figure | ~4 ms before explicit WMI properties/OEM additions. |
| SYSTEM INFORMATION | No current figure | ~78 ms with licensing WMI fallback; firmware-key path differs. |
| RAM MODULES | Direct type 17 merged, no new figure | ~2 ms formerly WMI; current direct timing unverified. |
| CPU | 1038 → 0 ms (integer-ms measurement) | Direct path below ms resolution here; fallback still can take ~1 s. |
| TPM MODULES | 554 → 292 ms | ~0.3 s on one tested Intel TPM; fallback differs. |
| USB DEVICES | No current figure | ~3 ms before hub serial enrichment; whole hub scan wait capped at 750 ms. |
| GPU INFO | No current figure | ~30 ms with NVIDIA; other vendors/fallbacks unverified. |
| MONITOR INFORMATION | No current figure | ~3 ms before expanded EDID enrichment. |
| NETWORK ADAPTERS (NIC's) | 123 ms; candidate 120 ms rejected as noise | ~0.1 s; existing WMI path retained. |
| BLUETOOTH ADAPTERS | 349 → 52 ms | ~50 ms empty-radio path, not a successful radio benchmark. |
| AUDIO DEVICES | Round 2: 11 ms five-run median (2026-10-04) | New Rust-only section; synchronous COM/topology and shared SetupAPI snapshot costs vary. |
| ARP INFO/CACHE | Native 2–3 ms; `arp.exe` 43 ms (2026-10-03 entry) | Milliseconds native, tens of ms process; cache varies. |

## DISK DRIVES

[Provider](src/hw/disk/mod.rs), [sources](src/hw/disk/sources.rs), [tree/ID conversion](src/hw/disk/formatting.rs), [storage helper](src/win/storage.rs).

```text
Device ID
--------------------------------------------------
└── PHYSICALDRIVE0
    ├── Drive: C
    │   └── Volume-SN: 7C29D4A6
    ├── Model: Samsung SSD 980 PRO 1TB
    ├── Serial: 0025_38C8_6B14_79A2.
    ├── NVMe Controller Serial: S5GXNF0R913742K
    ├── NVMe Namespace EUI-64: 002538C86B1479A2
    ├── NVMe Namespace NGUID: 6E8493F172B54CA091D8E2603F7A4B65
    ├── Hardware ID: SCSI\DiskNVMe____Samsung_SSD_980_2B2Q
    ├── Firmware: 2B2QGXA7
    ├── Adapter Serial: S5GXNF0R913742K
    ├── UniqueId (IOCTL): 00:25:38:C8:6B:14:79:A2
    ├── UniqueId (IOCTL) decoded: <empty>
    ├── UniqueId (WMI): 30:30:32:35:33:38:43:38:36:42:31:34:37:39:41:32
    ├── UniqueId (WMI) decoded: 002538C86B1479A2
    ├── Partition Style: GPT | Disk GUID: 4E67A291-AB36-4F82-976D-C51840B79E23
    └──   Partition GUID: 80D942C7-651B-4D30-AE82-17C9B6435F28
```

| Label | Meaning | ID | Appears when |
|---|---|---|---|
| Device ID / `PHYSICALDRIVE{n}` | Windows physical-device path with `\\.\` removed | No | Each WMI disk; null → `Unknown Device`. |
| Drive | Associated drive letter, without colon | No | One pair per letter, sorted; empty pair when none. |
| Volume-SN | Filesystem volume serial, uppercase hex | Yes | Each mapped volume; empty placeholder unmarked when no volumes. |
| Model | Windows model string | No | Always; null → `Unknown Model`. |
| Serial | Windows disk serial, trimmed | Yes | Always; null → `Unknown Serial`; empty stays empty. |
| NVMe Controller Serial | Controller SN, stripped of outer space/NUL padding | Yes | NVMe Identify succeeds with a nonempty valid ASCII serial. |
| NVMe Namespace EUI-64 / NGUID | Namespace bytes, uppercase hex without separators | Yes | Corresponding field is nonzero; independently optional. |
| NVMe Identify Controller | `Unavailable ({E})` or `Empty serial` status | No | After the intact tree, only when the reported bus is NVMe and its controller request fails or returns a padded-empty serial. |
| NVMe Identify Namespace | `Unavailable ({E})` status | No | After the intact tree, only when the reported bus is NVMe and its namespace request fails. All-zero namespace fields remain absent. |
| Hardware ID | First SetupAPI hardware ID, or lookup error text | Yes for ID; no for error | Nonempty map match, or map-level failure. |
| Firmware | Windows firmware revision, trimmed | No | Always; empty allowed. |
| ATA Serial (Identify) | ATA words 10–19, word-byte-swapped printable ASCII, outer space/NUL padding removed | Yes | Valid ATA/SATA Identify reply with a nonempty, non-placeholder serial. |
| ATA Model (Identify) / ATA Firmware (Identify) | ATA words 27–46 / 23–26, independently decoded with the same string rules | No | Corresponding string is nonempty and valid; context fields, not unit identity. |
| ATA WWN | ATA words 108–111 decoded as a 64-bit number, 16 uppercase hex digits | Yes | Words 84 and 87 have valid `01` top bits and WWN bit 8 set; value is neither zero nor all ones. |
| ATA Identify | `Unavailable ({E})` status | No | Both native ATA sources fail on an ATA/SATA bus; after the intact tree. |
| ATA field status | `{Label}: Unavailable ({E})` or `ATA Identify Serial: Empty or placeholder serial` | No | A field has malformed text, or the serial is padded-empty/a recognized placeholder. Other successful fields remain. |
| Adapter Serial | Storage-provider `AdapterSerialNumber`, trimmed; separate from Windows/controller serial | Yes | Nonempty field matched by the existing numeric disk-index join; after Firmware/optional ATA fields and before UniqueId details. |
| UniqueId (IOCTL) | Selected storage identifier as colon-separated bytes | Yes | Query returns a nonempty selected identifier. |
| UniqueId (IOCTL) decoded | ASCII-compatible rendering; otherwise `<empty>` | Yes only for nonempty decoded value | With the preceding IOCTL line. |
| UniqueId (WMI) / decoded | Storage UniqueId converted to bytes / original string | Yes | Nonempty UniqueId matched by disk index. |
| Partition Style | `GPT` or `MBR` | No | Recognized layout. |
| Disk GUID / Disk Signature | GPT GUID or MBR `0x` + 8 hex digits | Yes | With layout; alternative MBR line: `Partition Style: MBR | Disk Signature: 0x6B28D941`. |
| `  Partition GUID` | GPT partition identity, in layout order | Yes | Each nonzero GPT partition GUID; label has two leading spaces. |

**Sources/order.** Inventory, path, model, Windows serial, firmware and index: `root\cimv2:Win32_DiskDrive`, selecting only `DeviceID, Model, SerialNumber, FirmwareRevision, Index, PNPDeviceID`; there is no replacement inventory source. Hardware ID: cached present SetupAPI `SPDRP_HARDWAREID`, keyed by `PNPDeviceID`. Volumes: `GetLogicalDrives` → fixed/removable `GetDriveTypeW`, `IOCTL_STORAGE_GET_DEVICE_NUMBER` plus `IOCTL_VOLUME_GET_VOLUME_DISK_EXTENTS`, then `GetVolumeInformationW`; ambiguous/error mappings fall back per letter to `Win32_LogicalDisk` and the `Win32_LogicalDiskToPartition` / `Win32_DiskDriveToDiskPartition` associations. This retains spanned-volume associations and every letter without replacing filesystem identity.

Storage UniqueId and Adapter Serial: `root\Microsoft\Windows\Storage:MSFT_PhysicalDisk`, selecting `DeviceId, UniqueId, AdapterSerialNumber`; fields are retained independently. Bounded PowerShell `Get-PhysicalDisk` supplies only UniqueId on WMI failure (empty success does not fall back); its command and JSON parser are unchanged. GUID strings convert using mixed-endian GUID bytes; other strings use the low byte of each UTF-16 unit. Independently, `IOCTL_STORAGE_QUERY_PROPERTY/StorageDeviceIdProperty` ranks first ASCII, then nonzero binary 8/16-byte IDs, then the first remaining descriptor; association is not a selection filter. Layout comes from `IOCTL_DISK_GET_DRIVE_LAYOUT_EX`. NVMe enrichment first checks `StorageDeviceProperty.BusType`, then queries protocol-specific Identify Controller (CNS 1) and Namespace (CNS 0) separately. Each group returns `IdentifyOutcome::Ok`, `Failed`, `Empty`, or `NotAttempted { bus }`; one failed query cannot discard the other's identities. Windows serial is always retained. Namespace SubValue remains zero; multi-namespace association is unverified.

**Admin/timing/limits.** Admin required; physical queries use `GENERIC_READ`, except the ATA passthrough fallback described below. Elevated main-PC disk medians on 2026-10-04: supplied baseline 58 ms, final WP-A1 capture 27 ms (five samples per run; an earlier WP-A1 run measured 44 ms; no general speed claim). Storage calls bound the wait to 5 s each; an underlying synchronous IOCTL can continue. Get-PhysicalDisk has 15 s. USB/RAID/bridge support varies; namespace UUID descriptors are unbuilt. Existing unique-ID/layout unsupported codes 1/21/50/1112, plus unique-ID code 87, stay diagnostic-only. NVMe Identify failures, including unsupported requests, append `NVMe Identify Controller: Unavailable ({E})` / `NVMe Identify Namespace: Unavailable ({E})` only after an NVMe bus report; padded-empty controller serial appends `NVMe Identify Controller: Empty serial`. Non-NVMe skips and a failed bus probe stay diagnostic-only. Other failures follow the intact tree as `{Label}: {E}`; volume failures use `Drive {X}: Unavailable ({E})`, `Drive letters: {E}`, or `UniqueId (WMI): {E}`.

**Diagnostics/verification.** `.diag.txt` helper records include physical path, numeric reported BusType (or `unknown`), property, CNS/SubValue, returned buffer size (or `unknown` when no successful IOCTL reply is available), elapsed ms, and class (`ok`, `empty`, `not-attempted`, `timeout`, `access-denied`, `unsupported`, `absent`, `malformed`, `failed`). These telemetry records use the existing Error-shaped channel; their `class` determines whether a failure actually occurred. No serial bytes are included. Main-PC NVMe success and USB skip were exercised; deleting the sole added Adapter Serial line leaves a byte-identical baseline section. Adapter Serial is marked through the detail renderer's `id_value`; a temporary local check exercised that renderer and the shared masked text/JSON export with a fabricated serial, including the four-character minimum, whole-token boundaries and case-sensitive matching. **Untested hardware/fallback paths:** RAID/VMD, Storage Spaces, multi-namespace NVMe, Windows 10, Storage WMI/PowerShell failure, and NVMe access-denied/unsupported/timeout/malformed/empty replies.

**AD:** 01–06, 46, 74, 78–80, 98–104.

**ATA source priority/order.** Reuse the validated `StorageDeviceProperty.BusType` from the NVMe gate; no extra bus query. Only `BusTypeAta` / `BusTypeSata` permit an ATA command. First query `StorageDeviceProtocolSpecificProperty` with `ProtocolTypeAta` / `AtaDataTypeIdentify` and a 512-byte payload request. On IOCTL, descriptor, payload-bound or integrity failure, retain its diagnostic and try `IOCTL_ATA_PASS_THROUGH` with the fixed read-only IDENTIFY DEVICE command `0xEC`. Only that fallback opens `GENERIC_READ | GENERIC_WRITE`; no DATA_OUT, DMA or other command is sent. Validate the returned header, data offset, exact 512-byte transferred length, returned-buffer bounds and completed task-file status (DRDY, no BSY/DF/DRQ/ERR). If Identify word 255 advertises signature `0xA5`, require the whole-block checksum to pass. Per-field string errors do not erase other fields. Empty strings stay absent; serial placeholders `UNKNOWN`, `UNKNOWN SERIAL`, `NONE`, `N/A`, `NA`, `NOT SPECIFIED`, `NOT AVAILABLE`, `DEFAULT STRING`, `TO BE FILLED BY O.E.M.`, all-zero digits and all-F strings are skipped. ATA details appear after Windows Firmware, in serial/model/firmware/WWN order, before Adapter Serial and UniqueId details; Windows values are never replaced. Existing detail rendering marks only ATA serial and WWN with `id_value`, using the shared four-character minimum, whole-token, case-sensitive mask rules. Fabricated lines:

```text
    ├── ATA Serial (Identify): S6PUNF0R812345X
    ├── ATA Model (Identify): Samsung SSD 870 EVO 1TB
    ├── ATA Firmware (Identify): SVT02B6Q
    ├── ATA WWN: 5002538EA1B2C3D4
```

**ATA limits/verification.** Each source has a separate 5 s caller-wait bound; passthrough also requests a 3 s driver timeout. A timed-out synchronous worker retains its handle/buffers until the driver returns. Non-ATA/SATA buses are diagnostic-only skips; USB/SAT bridges and RAID/VMD vendor backends are excluded. An unknown/failed bus probe permits no ATA command. The protocol source's failure remains diagnostic-only when passthrough succeeds; failure/absent/unsupported/access-denied/timeout/malformed classes remain distinct. ATA parser fixtures are fabricated 512-byte blocks and cover string word order, partial string failures, serial placeholders, WWN support/validity/sentinels, optional integrity, descriptor bounds, task-file errors and short transfers. Elevated main-PC WP-B1 verification on 2026-10-04 exercised the SATA fallback: preferred protocol query returned unsupported code 1, passthrough returned 560 bytes in 77 ms, and serial/model/firmware matched the existing Windows fields. The three added lines are the only byte differences from WP-A1; removing them restores the complete DISK DRIVES report exactly, including its NVMe disk and CRLF. A separate elevated read-only Identify capture confirmed valid WWN support/active words but all-zero WWN words, so no WWN line is expected. WP-B1 disk median was 54 ms versus WP-A1's 27 ms, five full-machine samples each; single-run IOCTL latency and cross-run medians differ and imply no general speed claim. Captures remain in session temp. Masking was checked by tracing both new identity fields through the existing detail renderer's `id_value` and shared mask rules; no new CLI masked mode was added. **Untested ATA hardware/fallback paths:** successful preferred protocol query, a drive with nonzero WWN, legacy parallel ATA, Windows 10, and real access-denied/absent/timeout/task-file-error/malformed/integrity/placeholder replies. USB/SAT and RAID/VMD remain out of scope.

**Not built:** Namespace UUID/all VPD descriptors — more device identities; strict descriptor validation and association cost.

## MOTHERBOARD

[Provider](src/hw/motherboard.rs), [firmware](src/win/firmware.rs).

```text
Manufacturer: ASUSTeK COMPUTER INC.
Product: PRIME Z690-P
Version: Rev 1.xx
SerialNumber: 221109684732518
Asset Tag: INV2024B7319
Location: Mainboard
Source: SMBIOS (direct)
```

| Label | Meaning | ID | Appears when |
|---|---|---|---|
| Manufacturer / Product | Board maker/model | No | Always in chosen path. |
| Version | Firmware board revision | No | Direct path, even empty. |
| SerialNumber | Board serial claim | Yes | Always in chosen path, even empty. |
| Asset Tag | Board asset string | Yes | Direct, nonempty. |
| Location | Board location string | No | Direct, nonempty. |
| Source | Literal `SMBIOS (direct)` | No | Direct path. |
| Model | WMI model | No | WMI path only, even empty. |
| SKU | WMI stock-keeping value | Yes | WMI path only, even empty. |

**Sources/order.** Shared `GetSystemFirmwareTable(RSMB, 0)`, type 2, offsets 4/5/6/7/8/0x0A → `root\cimv2:Win32_BaseBoard` if the final direct manufacturer is empty or firmware fails. Later records overwrite fields whose offsets exist. WMI prints Manufacturer, Product, Model, SKU, SerialNumber for each row, without separators. Direct reading avoids WMI while preserving its fallback inventory.

**Admin/timing/limits.** Admin app; firmware API itself needs no special privileged device handle. Historically sub-ms, shared initialization varies. Multiple boards collapse into one direct field set; expanded topology unverified. Empty WMI after a firmware error exposes the firmware error; otherwise an empty result can leave a blank body.

**AD:** 01–03, 09, 46. **Not built:** additional type-2 records/board-chassis handles — inventory topology; small decoding cost, presentation/primary-board compatibility work. PCIe Device Serial Number (DSN) — optional endpoint identity, not motherboard serial; supported PCI configuration access and possibly a narrow driver require research.

## CHASSIS

[Provider](src/hw/chassis.rs), [firmware](src/win/firmware.rs).

```text
Manufacturer: ASUSTeK COMPUTER INC.
Type: Desktop
Version: 1.2
Serial Number: CH2411B728493
Asset Tag: ENC2024K6198
SKU: DESK-Z690-ATX
```

| Label | Meaning | ID | Appears when |
|---|---|---|---|
| Manufacturer | Enclosure maker | No | Required for any normal chassis fields. |
| Type | Type code with lock bit removed, decoded name or `Type {n}` | No | Type byte exists and manufacturer nonempty. |
| Version | Enclosure revision | No | Nonempty. |
| Serial Number / Asset Tag | Enclosure serial/asset claims | Yes | Nonempty. |
| SKU | Chassis SKU | Yes | SMBIOS ≥2.7, valid variable-length field, meaningful and different from existing chassis strings. |

**Sources/order.** RSMB type 3 only; no WMI fallback. Legacy fields use later present offsets. SKU is associated with the last type-3 record and follows its contained-element array; placeholders/control characters are rejected. Direct firmware is already the source.

**Admin/timing/limits.** Admin app; firmware read. Historically sub-ms; latest SKU timing unverified. Missing manufacturer gives `Chassis information not available.` A firmware failure adds `Error retrieving CHASSIS information: {E}`. Multi-enclosure inventory is collapsed and unverified.

**AD:** 01–03, 09, 46, 73, 77. **Not built:** separate extra enclosures/type-3 topology — exposes multi-chassis systems; decoding and stable group-order cost.

## (SM)BIOS

[Provider](src/hw/bios.rs), [firmware](src/win/firmware.rs), [WMI](src/win/wmi.rs).

**WQL projection (WP-A6).** Both production queries already select only needed properties; the live-capture helper now does too. Source priority, independent query failures, last-row behavior, and report text are unchanged.

**WP-A6 verification (2026-10-04).** Elevated section body byte-identical to the supplied baseline; five-run median 5 → 5 ms on this dev machine. Cross-OEM and failed-query paths were not exercised.

```text
Manufacturer: American Megatrends Inc.
Vendor: ASUSTeK COMPUTER INC.
Version: 2802
SMBIOS Version: 2802
Release Date: 09/27/2023
UUID: C736B9A2-154D-4E80-9A62-D8714F0B35C9
IdentifyingNumber: SYS24K731862
SerialNumber: BIOS24R862519
System Manufacturer: ASUSTeK COMPUTER INC.
System Product: PRIME Z690-P
System Serial: SYS24K731862
System SKU: ASUS-MB-Z690
System Family: Desktop
System Version: 1.2
OEM String (0x0038, 1): Asset Tag: INV24D8517
```

| Label | Meaning | ID | Appears when |
|---|---|---|---|
| Manufacturer / Version | BIOS vendor/version | No | Always, possibly empty. |
| Vendor | System-product vendor | No | Always, possibly empty. |
| SMBIOS Version | **WMI `SMBIOSBIOSVersion`, a BIOS version string**, not table major.minor | No | Always, possibly empty. |
| Release Date | Type-0 release-date string | No | Nonempty direct value. |
| UUID | System UUID | Yes | Always, possibly empty. |
| IdentifyingNumber | WMI system-product identity | Yes | Always, possibly empty. |
| SerialNumber | WMI BIOS serial | Yes | Always, possibly empty. |
| System Manufacturer / Product / Family | Firmware system strings | No | Nonempty direct values. |
| System Serial / System SKU | Firmware system serial/SKU | Yes | Nonempty direct values. |
| System Version | Type-1 revision | No | Meaningful optional value, excluding known placeholders. |
| OEM String (0xHHHH, n) | Explicitly labeled type-11 serial/asset/SKU/part number | Yes, value only | Accepted text; trimmed value after `:`/`=` is marked, not the full line. |

**Sources/order.** Manufacturer/Version: type 0 → `Win32_BIOS.Manufacturer/Version` only when direct empty. Release Date: type 0 only. UUID: type 1 bytes → `Win32_ComputerSystemProduct.UUID` when direct empty. Vendor/IdentifyingNumber: `Win32_ComputerSystemProduct` only; SMBIOS Version/SerialNumber: `Win32_BIOS` only. Both WMI queries run independently with explicit selected properties in `root\cimv2`. Other System fields/version: type 1 only. OEM strings: type 11 only. This retains WMI fields whose semantics have no proven direct equivalent.

**Admin/timing/limits.** Admin app; firmware plus WMI. Historically a few ms; latest enrichment timing unverified. UUID swaps the first 4/2/2 bytes for every SMBIOS version, including zero/FF sentinels. Repeated direct records overwrite present fields; last WMI row wins. One failed WMI query blanks only its own fields, retaining direct/other-query fields, then adds `WMI query failed: {class}: {E}`. OEM text must use an accepted explicit label, contain no controls and have a useful value not already in the legacy field sets; arbitrary OEM messages are omitted. Cross-OEM behavior unverified.

Accepted OEM labels (case-insensitive, trimmed): `serial`, `serial number`, `serialnumber`, `s/n`, `sn`, `asset tag`, `sku`, `part number`, `p/n`. Optional System Version/OEM values/chassis SKU reject controls, empty or only `0`/`F`/hyphens/spaces, and known placeholders: default string, to be filled by O.E.M./OEM, not specified/applicable/available, unspecified, unknown, none, n/a, system version/SKU/family/serial number, chassis serial number, no asset tag. This filtering does not rewrite legacy fields.

**AD:** 01–03, 09, 10, 46, 75, 76. **Not built:** type-45 firmware inventory/BIOS characteristics — component context, not replacement BIOS identity; typed decoding cost. Further WMI reduction — small latency gain; requires equality per field, never substitute table version for `SMBIOSBIOSVersion`.

## SYSTEM INFORMATION

[Provider](src/hw/system.rs), [registry](src/win/registry.rs), [firmware](src/win/firmware.rs), [time](src/win/time.rs).

**WQL projection (WP-A6).** The licensing fallback selects only `OA3xOriginalProductKey`; all rows remain in provider order, including null/empty keys. MSDM priority, Product ID fallback, identifier marking, and per-item error text are unchanged.

**WP-A6 verification (2026-10-04).** Elevated section body byte-identical to the supplied baseline; licensing WMI fallback exercised with MSDM absent. Five-run median 85 → 8 ms on this dev machine. Multiple-row licensing and access-denied/malformed fallback paths were not exercised.

```text
Windows Product Key: 9QH4V-2WJ7R-K6D8M-P3X5Y-TNFBC
Serial Number (Product ID): 00330-80000-00000-AB719
Machine GUID: b27f41e8-902c-4e63-a185-7d36c04982ba
Hardware Profile GUID: {71A25E94-D6B8-4C02-9F31-826C50A7B493}
Install Date: 2024-03-19 14:27:36
```

| Label | Meaning | ID | Appears when |
|---|---|---|---|
| Windows Product Key | Firmware OA3 key | Yes | Nonempty key from chosen source. |
| Activation Status | Literal `Not activated or using Volume License` | No | WMI returns an empty/null key; not a verified activation diagnosis. |
| Serial Number (Product ID) | Windows ProductId / OS SerialNumber | Yes | Successful nonempty source. |
| Machine GUID | Windows installation GUID | Yes | Registry value nonempty; not firmware UUID. |
| Hardware Profile GUID | Profile **0001** GUID | Yes | Registry value nonempty; not necessarily current profile. |
| Install Date | Signed Unix seconds converted to local time | No | Successful type read and time conversion. |
| Any item label with `Unavailable (E)` | Unresolved item error | No | All its sources fail. |

**Sources/order.** Key: `GetSystemFirmwareTable(ACPI, MSDM)` with length/type/checksum validation → `root\cimv2:SoftwareLicensingService.OA3xOriginalProductKey`. Product ID: HKLM `SOFTWARE\Microsoft\Windows NT\CurrentVersion\ProductId` → `Win32_OperatingSystem.SerialNumber`. Machine GUID: HKLM `SOFTWARE\Microsoft\Cryptography\MachineGuid`. Hardware Profile GUID: HKLM `SYSTEM\CurrentControlSet\Control\IDConfigDB\Hardware Profiles\0001\HwProfileGuid`. Install Date: CurrentVersion `InstallDate`, DWORD → QWORD → signed integer string. Registry reads use the 64-bit HKLM view. Firmware/registry first avoids slow WMI without confusing hardware and OS identities.

**Admin/timing/limits.** Admin app; registry ACLs still apply. Historically tens of ms with licensing fallback; current timing unverified. No installed-key decoder or actual license-state query. Missing MSDM is normal on many PCs; WMI can supply multiple rows/keys. Empty GUIDs are omitted, not replaced. Per-item errors preserve other lines.

**AD:** 01–03, 46. **Not built:** specifically documented UEFI variables — boot/security or vendor context, no universal HWID; privilege, firmware availability and semantics research cost. No arbitrary variable scraping.

## RAM MODULES

[Provider](src/hw/ram.rs), [firmware](src/win/firmware.rs).

**WQL projection (WP-A6).** The incomplete-SMBIOS fallback selects only `DeviceLocator`, `Manufacturer`, `PartNumber`, `Capacity`, and `SerialNumber`. Inventory/order, unique-match overlays, null-capacity handling, UTF-16 table widths, and serial marking are unchanged.

**WP-A6 verification (2026-10-04).** Elevated section body byte-identical to the supplied baseline; five-run median 0 → 0 ms on this dev machine. Complete SMBIOS won, so the narrowed WMI fallback remains untested on real hardware.

```text
DeviceLocator   Manufacturer PartNumber     Capacity SerialNumber
-----------------------------------------------------------------
DIMM_A2         Kingston     KF432C16BB1/16 16 GB    8C31D7A2
DIMM_B2         Kingston     KF432C16BB1/16 16 GB    8C31D7B9
```

The example illustrates columns; actual padding is calculated from the rows (minimum widths 15/12/10/8/12 UTF-16 units), including padding after the last column.

| Column | Meaning | ID | Appears when |
|---|---|---|---|
| DeviceLocator | Firmware/WMI slot label | No | Each selected module row. |
| Manufacturer | Module maker string | No | Each row; empty possible on fallback. |
| PartNumber | Module part/model string | No | Each row. |
| Capacity | Bytes / 2^30 rounded ties-to-even to whole `GB` (legacy label) | No | Each row; unknown/null fallback can be `0 GB`. |
| SerialNumber | Module serial claim | Yes when nonempty | Each row; empty string unmarked. |

**Sources/order.** Shared type 17 (locator 0x10, maker 0x17, part 0x1A, serial 0x18, size 0x0C/extended 0x1C) if every field of every selected record is complete → `root\cimv2:Win32_PhysicalMemory`. On incomplete firmware, WMI owns inventory/order; direct fields overlay only a unique exact locator or serial match with no conflicting locator or reused match. No position-based join. Empty type-17 slots (size zero) are skipped; unknown/extended/KB sizes are decoded before formatting. Direct source removes WMI when equivalent.

**Admin/timing/limits.** Admin app; firmware read. Current direct timing unverified; old WMI ~2 ms. These are firmware-published fields, not SPD EEPROM reads. No rows gives `No RAM modules detected.` WMI conversion failures become collector errors. Unusual/ambiguous DIMM layouts rely on WMI; cross-platform coverage unverified.

**AD:** 01–03, 45, 46. **Not built:** bank locator, asset tag, configured/rated speed — useful slot/profile context already in type 17; small decoding/presentation cost. Raw SPD through a narrow driver — manufacturing bytes/profiles independent of SMBIOS publication; controller/DDR-generation access, arbitration, driver security and distribution cost; untested.

## CPU

[Provider](src/hw/cpu.rs), [firmware](src/win/firmware.rs), [registry](src/win/registry.rs).

```text
Name: 12th Gen Intel(R) Core(TM) i7-12700K
ProcessorId: BFEBFBFF00090672
SerialNumber: CPU24J731982

CPUID Vendor: GenuineIntel
CPUID Signature (decoded): Family 6, Model 151, Stepping 2
```

| Label | Meaning | ID | Appears when |
|---|---|---|---|
| Name | Processor name from registry/WMI | No | Direct processor or each WMI row. |
| ProcessorId | Firmware/WMI 64-bit processor identity, not unique chip serial | Yes when nonempty | Each processor row. |
| SerialNumber | Firmware/WMI serial or OEM placeholder | Yes when nonempty | Direct path; WMI only when property is non-null (empty still shown). |
| CPUID Vendor | Leaf-0 vendor bytes (EBX, EDX, ECX) | No | Always, after blank line. |
| CPUID Signature (decoded) | Leaf-1 Family/Model/Stepping | No | Maximum leaf permits leaf 1. |
| CPUID Serial Number | Leaf-3 EDX:ECX as 16 hex digits | Yes | Maximum leaf ≥3 and result nonzero. |

**Sources/order.** Direct path requires exactly one populated, enabled type-4 CPU of processor type 3, nonzero/non-FF ID and nonempty serial. Name comes from HKLM `HARDWARE\DESCRIPTION\System\CentralProcessor\{n}\ProcessorNameString`; all numeric logical-processor subkeys must have the same nonempty name. Otherwise use `root\cimv2:Win32_Processor` preserving row order. ProcessorId is the little-endian type-4 qword as uppercase X16; live CPUID EDX:EAX must not replace it. CPUID leaves 0/1/3 are independent and still print if WMI fails.

**Admin/timing/limits.** Admin app; registry/firmware/CPUID user-mode sources. Latest direct result 0 ms at integer resolution, previously ~1 s WMI. Multi-socket, disabled/unclear states and missing serial force WMI; successful multi-socket behavior is unverified. Leaf 3 has no PSN feature-bit check: any emitted value is unverified as a unique serial. OEM placeholders remain visible/marked.

**AD:** 01–03, 45, 46 (partial-output conventions). **Not built:** socket/manufacturer/part/asset metadata — per-socket context from type 4; inventory/order matching cost. PPIN — platform-specific inventory number; privileged MSR interface/firmware locks and driver cost, untested.

## TPM MODULES

[Provider](src/hw/tpm.rs), [TPM helper](src/win/tpm.rs).

**WQL projection (WP-A6).** `Win32_Tpm` selects `ManufacturerIdTxt`, `ManufacturerId`, `ManufacturerVersion`, `SpecVersion`, `IsEnabled_InitialValue`, and `IsActivated_InitialValue`. The WMI wrapper retains `__PATH`/`__RELPATH` for both method calls; first-object behavior, initial-state fallbacks, nonzero method errors, independent EK collection, and identifier marking are unchanged.

**WP-A6 verification (2026-10-04).** Elevated section body byte-identical to the supplied baseline; WMI status and native EK sources retained with no method/object-path diagnostics. Five-run median 279 → 310 ms on this dev machine; no TPM speed gain established. Absent/disabled TPM, initial-state and PowerShell fallbacks, nonzero method codes, TPM 1.2, and a second TPM implementation were not exercised.

```text
TPM: ENABLED
TPM Manufacturer: IFX
TPM Version: 7.85.4555.0
TPM Spec Version: 2.0, 0, 1.59
Sha256 Hash: 73a19d6c8f204be592d37a06c4185f9b2e641c7a908d53f68b12e074a9c635d2
Serial Number: 03A761B29E845C17D0F638
Thumbprint: 49B617C230A8E56D91F47B0C3D825AE679104FC2
Issuer: CN=IFX TPM EK CA, O=Infineon Technologies AG
```

| Label | Meaning | ID | Appears when |
|---|---|---|---|
| TPM | `ENABLED`, `DISABLED`, or `UNKNOWN (IsEnabled failed: 0xHHHHHHHH)` | No | Present status; unknown for nonzero TPM method return. |
| TPM Manufacturer | ManufacturerIdTxt or numeric `0x` + 8 hex digits | No | Nonempty WMI value. |
| TPM Version / TPM Spec Version | Manufacturer firmware/spec strings | No | Nonempty WMI values; PowerShell status fallback omits them. |
| Sha256 Hash | SHA-256 of default EK public-key encoding (native lowercase hex) | Yes | Enabled and usable EK output. |
| Serial Number | EK certificate serial, **not chip serial** | Yes | Usable certificate field. |
| Thumbprint | EK certificate SHA-1 thumbprint | Yes | Usable certificate field. |
| Issuer | Literal CN/O components, in that order | No | At least one usable component. |

**Sources/order.** Status: first `Win32_Tpm` in `root\cimv2\Security\MicrosoftTpm` → PowerShell `Get-Tpm`. `IsEnabled`/`IsActivated` methods are read; only enabled controls the visible status. Missing/unsupported Boolean output can use the corresponding initial-state property; a returned nonzero TPM method code remains unknown. EK, only when enabled: Microsoft Platform Crypto Provider (`NCryptOpenStorageProvider`, `PCP_EKPUB`, `PCP_EKNVCERT`, `PCP_EKCERT`) plus crypt32 → full `Get-TpmEndorsementKeyInfo -Hash 'Sha256' | Format-List` fallback. Native path accepts RSA public blobs, hashes PKCS#1 DER, requires exactly one manufacturer certificate and at most one identical additional certificate, and rejects long/multiline issuer formatting. This preserves legacy encodings and ambiguous certificate behavior rather than inventing a different native result.

**Admin/timing/limits.** Admin app; TPM/EK policies can deny access. Latest ~292 ms. PowerShell processes each have 15 s; provider still 60 s. Native parity was tested on one Intel TPM; second TPM implementation gate is unmet. ECC/ambiguous stores use fallback; their live parity is unverified. Fallback parser retains only the next nonempty certificate-field line; repeated keys overwrite. No key possession, certificate binding/chain or attestation verification. Absent status gives `TPM OFF`; unknown/empty collection uses `Unable to retrieve TPM information`, optionally `: {E}`. EK failures can give `Unable to retrieve detailed TPM information`, optionally `: {E}`.

**AD:** 01–03, 11, 46. **Not built:** type-43 firmware metadata/TBS capability cross-check — extra vendor/version context; command/state semantics and TPM1.2 compatibility cost. ECC/complete multi-certificate native EK and certificate validation — broader coverage/authenticity evidence; encoding/order and second-TPM testing cost.

## USB DEVICES

[Provider](src/hw/usb.rs), [SetupAPI](src/win/setupapi.rs), [hub helper](src/win/usbhub.rs).

```text
Device: SanDisk Ultra USB Device
Serial: 4C530001281019105732
Serial (device): 4C530001281019105748
Device Manufacturer: SanDisk
Device Product: Ultra
Container ID: {B9E25D41-73AF-4CA8-9F26-2D805671A493}
----------------------------------------
Device: USB Receiver
Serial (device): 83917A5E
Device Manufacturer: Logitech
Device Product: USB Receiver
Container ID: {4A8BC390-2586-4E71-B642-C37D90E51A28}
```

| Label | Meaning | ID | Appears when |
|---|---|---|---|
| Device | OS friendly name → description; possibly empty/error | No | Each accepted present devnode. |
| Serial | Last component of USB/USBSTOR instance ID | Yes | Prefix begins `USB`, tail contains none of `&`, `.`, `{`; even empty/zero tails accepted. |
| Serial (device) | USB string descriptor serial | Yes | Exact driver-key association and byte-different from the instance tail, including generated tails. Match/failure/timeout adds no line. |
| Device Manufacturer / Device Product | Device-reported iManufacturer / iProduct strings, separate from the OS name | No | Nonzero descriptor index, nonempty valid UTF-16 without control characters, successful exact driver-key/port recheck. |
| Container ID | Windows PnP devnode's container GUID, in braces | Yes | Accepted present devnode has a non-null GUID-typed `DEVPKEY_Device_ContainerId`; firmware/bus-supplied versus Windows-generated origin is not determined. |

**Sources/order.** Present all-class SetupAPI scan: instance ID → `SPDRP_FRIENDLYNAME` → `SPDRP_DEVICEDESC`. Independently enumerate hub interfaces; use `IOCTL_USB_GET_NODE_INFORMATION`, `IOCTL_USB_GET_NODE_CONNECTION_INFORMATION_EX`, driver-key-name query and `IOCTL_USB_GET_DESCRIPTOR_FROM_NODE_CONNECTION`. Read serial first, then manufacturer and product with the same first advertised LANGID; associate only to exact devnode `SPDRP_DRIVER`, recheck connection/driver key after each string, reject ambiguous keys. Each verified string survives failures in another string read. Instance-derived serial stays primary because many devices have no readable descriptor serial. Generated instance tails still never produce `Serial`; such devnodes get a group only when their exact driver-key match supplies a valid byte-different device serial. No positional or VID/PID-only association is used. `SetupDiGetDevicePropertyW` reads ContainerID by exact present instance ID; missing/null properties are omitted, malformed types/sizes and other failures stay diagnostic-only and do not remove device strings.

**Admin/timing/limits.** Admin app; hub handles use `GENERIC_WRITE` for read-only descriptor IOCTLs, as in USBView. Elevated WP-A3 capture on the dev PC (2026-10-04): median 4 → 13 ms across five samples; three existing groups retained, two manufacturer/product pairs and three ContainerIDs added. Whole enrichment wait ≤750 ms (synchronous worker can finish later; one busy worker blocks a second scan). Verified strings are delivered immediately, so a later blocking request cannot withhold an earlier field past the wait cap. Not every USB device is shown: generated instance tails without an exact, differing descriptor serial remain filtered. Generated-tail descriptor-backed groups, differing device serials, hot-unplug, ambiguous keys, malformed strings/GUIDs, denied/unsupported properties and stuck-worker paths were not exercised on this hardware. Composite/bridge paths vary. Empty result is a blank body. Partial enumeration then failure keeps groups and adds `Error: Unable to retrieve USB information` and `Error: {E}`. Name-read failures can display `Device: {E}` before Serial. Optional hub failures go to helper diagnostics; ContainerID access-denied/unsupported/malformed failures keep their distinct API codes/details in section diagnostics. Windows PnP ContainerID groups functions and is not asserted to be a raw firmware identity.

**AD:** 01–03, 07, 46, 71, 72, 93, 95–97. **Not built:** parent/composite-function inference beyond exact driver-key association; additional LANGID selection beyond the first advertised language.

## GPU INFO

[Provider](src/hw/gpu.rs), [NVIDIA helper](src/win/nvidia.rs).

```text
GPU 0
└── NVIDIA GeForce RTX 3070
    └── UUID: GPU-9e521d74-03ba-4c68-a27f-81d639b504ce

Board Serial Number: 032482719635
Serial Number: 032482719635
PDI: 08F47A2196BC3D50
Board Part Number: 900-1G141-2530-000
VBIOS Version: 94.04.3A.00.71

GPU 1
└── Intel(R) UHD Graphics 770
    ├── PCI\VEN_8086&DEV_4680&SUBSYS_86941043&REV_0C\3&74B19D2A&0&10
    └── Hardware ID: PCI\VEN_8086&DEV_4680&SUBSYS_86941043&REV_0C
```

| Line/label | Meaning | ID | Appears when |
|---|---|---|---|
| GPU n / tree name | NVIDIA index/name or appended WMI adapter index/name | No | Each selected adapter. |
| UUID | NVIDIA vendor-reported UUID | Yes | Successful NVIDIA source supplies UUID suffix. |
| Serial Number / GPU n Serial Number | NVML board/module serial, per GPU | Yes | NVML supplies a valid string that is neither empty, whitespace nor zero-only after trimming. |
| PDI / GPU n PDI | NVML physical device identifier, sixteen uppercase hexadecimal digits | Yes | Optional versioned PDI query succeeds on a supported GPU/driver. |
| Board Part Number / GPU n Board Part Number | NVML board part number, distinct from NVAPI BoardNum | Yes | NVML supplies a valid string that is neither empty, whitespace nor zero-only after trimming. |
| VBIOS Version / GPU n VBIOS Version | NVML firmware version | No | NVML supplies a nonempty valid string. |
| Board Serial Number / GPU n Board Serial Number | NVAPI 16-byte BoardNum, ASCII or separator-free uppercase hex | Yes for value; no for error | Proven complete unique PCI-bus match and meaningful bytes; multi-GPU also requires complete unique domain-zero PCI BDF joins to WMI rows. Single-GPU label stays unchanged; error can use `Unavailable (E)`. |
| Unlabeled PNP tree line | WMI PNPDeviceID | Yes | Each fallback/additional adapter. |
| Hardware ID | First SetupAPI hardware ID | Yes | Matching cached map entry. |

**Sources/order.** NVIDIA: NVML from System32 `nvml.dll` → `%ProgramW6432%\NVIDIA Corporation\NVSMI\nvml.dll` → `nvidia-smi.exe -L` at those locations. NVML enumerates every device with count/handle-by-index, then independently runtime-resolves name, UUID, serial, PDI, VBIOS and board-part queries. PDI uses the aligned versioned `nvmlPdi_v1_t` ABI (`sizeof | 1 << 24`). Missing symbols, `NOT_SUPPORTED`, access denial, malformed/empty strings and other statuses omit only the affected field and remain distinct diagnostics; name failure uses `Unknown`, UUID failure omits that leaf, handle failure retains other devices. Optional PCI query correlates NVAPI `GetBusId/GetBoardInfo` from System32 `nvapi64.dll`. All libraries are runtime-loaded. `root\cimv2:Win32_VideoController` always supplies other adapters or complete fallback; represented NVIDIA rows are suppressed by count/name matching, unmatched adapters retained. For multi-GPU board association, exact WMI PNP instance IDs open SetupAPI devnodes; bus/address DWORDs and the documented PCI location string prove domain-zero BDFs. Unrecognized/localized or nonzero-segment location strings fail the join conservatively. PNP IDs enrich through shared SetupAPI. Vendor API keeps NVIDIA UUID coverage; WMI retains other inventory.

**Admin/timing/limits.** Admin app; vendor driver required. Elevated single-GPU measurement (2026-10-04, WP-A2 round 2): GPU median 31 ms before and 31 ms after; the original GPU section body is byte-identical after excluding the new blank-separated PDI/VBIOS detail block. Consumer-board serial was zero-only and board part empty; both lines were omitted, each with one value-free `placeholder` diagnostic. Empty, whitespace and zero-only serial/board-part strings are placeholders (zero-only check after trimming); valid string bytes are retained. All serial/PDI/UUID/board-part/board-serial values use `id_value`; live UUID/PDI and fabricated optional values passed masking checks, preserving the four-character threshold, whole-token boundaries and case-sensitive matching. VBIOS is not an ID. Other-vendor timing unverified. `nvidia-smi` identity/PCI commands keep their parser/output and share one 15 s budget across all fallback paths; canonicalized case-insensitive executable paths are attempted once. Vendor DLL calls remain bounded only by the outer 60 s collection deadline. Zero/NUL/whitespace board values omitted; binary values become 32 hex digits. Matching requires complete GPU/bus lists, equal counts, unique bus IDs and zero PCI domains; a failed optional board value retains its proven bus so another matched GPU's board survives. All NVIDIA name/UUID groups remain byte-identical and together, then one empty line before the per-GPU detail block, then AD-14 extra adapters. Within the block, GPUs follow NVIDIA index order; each existing board serial line comes first with unchanged bytes, followed by Serial Number, PDI, Board Part Number and VBIOS Version when present. Exactly one NVIDIA GPU uses plain labels; two or more prefix every detail line with `GPU {n} `. NVML details still appear when a GPU has no board line. If the block has no lines, no empty line is added. Existing board-failure text and independent WMI fallback remain unchanged. No live AMD/Intel manufacturing UUID source implemented. **Untested hardware/paths:** real multi-GPU, no-NVIDIA, populated NVML serial/board part, missing-symbol/unsupported/access-denied APIs, nonzero PCI domains, localized location strings and duplicate-path/timeout `nvidia-smi` fallback. Scratch-only checks cover single/multi-GPU rendering, no-board/empty-block cases, placeholder parsing and masking. Empty inventory: `No GPU detected.` WMI enrichment failure after native output: `WMI query failed: Win32_VideoController: {E}`.

**AD:** 01–03, 12–16, 46, 110–115. **Not built:** AMD ADL/ADLX or D3DKMT UUID research — vendor inventory; optional APIs/hardware validation cost. DXGI LUID is session-local adapter identity, not a manufacturing UUID replacement.

## MONITOR INFORMATION

[Provider](src/hw/monitor.rs), [EDID parser](src/win/edid.rs).

```text
Count: 1 monitor(s) found:

Manufacturer: DEL
Model: DELL U2723QE
Serial Number: 8VJ6M42
Product Code: A1F4
Manufacturing Date: Week 18, 2024
EDID Serial (numeric): 1937468251
EDID Serial (text, registry): 8VJ6M47
EDID Product Code (registry): A1F5
EDID Manufacturer (registry): ACR
EDID Model (registry): XV272U
EDID Manufacturing Date (registry): Week 19, 2024
EDID Override Key: Present (registry; effective override not verified)
EDID Manufacturer OUI (block 1, CTA-861): 00-10-FA
EDID Model (block 1, CTA-861): DELL U2723QE
EDID Manufacturer (block 2, DisplayID): DEL
EDID Product Code (block 2, DisplayID): A1F4
EDID Serial (block 2, DisplayID): 1937468297
EDID Model (block 2, DisplayID): U2723QE
EDID Serial (block 2, DisplayID): 8VJ6M49
```

This deliberately illustrates differing WMI and cached registry values, not one verified physical monitor identity.

| Label | Meaning | ID | Appears when |
|---|---|---|---|
| Count | WMI row count or retained valid registry instances | No | Nonempty inventory; fallback adds `(from registry):`. |
| Manufacturer / Model | WMI maker/friendly name, or EDID maker/model descriptor | No | Nonempty WMI string; registry manufacturer always, descriptors when nonempty. |
| Serial Number | WMI text serial or EDID 0xFF descriptor | Yes | Nonempty decoded text. |
| Product Code | WMI ProductCodeID text | No | Nonempty WMI value; no unqualified fallback product-code line. |
| Manufacturing Date | WMI week/year | No | Both properties non-null. |
| EDID Serial (numeric) | EDID little-endian 32-bit serial, decimal | Yes | Neither 0, 0xFFFFFFFF nor 0x01010101. |
| Presence | `Not connected` | No | Registry fallback only, successful present-ID snapshot lacks this exact instance. |
| EDID Serial (text, registry) | Cached descriptor serial | Yes | Not exactly equal to a Serial Number already shown; normally absent in registry fallback because that serial is already printed. |
| EDID Product Code (registry) | 16-bit code as four uppercase hex digits | No | Missing/different from WMI; always in valid registry fallback. |
| EDID Manufacturer / Model (registry) | Cached manufacturer/model | No | WMI path only, missing/different; fallback already prints these unqualified. |
| EDID Manufacturing Date (registry) | Parsed week/year or `{year} (week unspecified)` | No | Valid date, missing/different from WMI; always in valid fallback. |
| EDID Model Year (registry) | EDID 1.4 week 255 model year | No | Valid model-year encoding; alternative to registry manufacture date. |
| EDID Override Key | Presence of exact-instance override key | No | Key opens; effective override is unverified. |
| EDID Serial (block {n}, DisplayID) | Product Identification little-endian UInt32 serial, or 1.x Product Serial Number ASCII descriptor | Yes | Valid block/descriptor; numeric zero or empty text omitted. Unlike the base parser, no other numeric placeholders are suppressed. |
| EDID Manufacturer (block {n}, DisplayID) | Three uppercase ASCII PNP manufacturer letters | No | DisplayID 1.x Product Identification tag 0x00, revision 0. |
| EDID Manufacturer OUI (block {n}, DisplayID) | Three-byte IEEE OUI, in stored first-to-third order | No | DisplayID 2.0 structure, Product Identification tag 0x20 revision 0; not zero/all-ones. Fabricated-context example: `00-10-FA`. |
| EDID Product Code / Model (block {n}, DisplayID) | Four-digit uppercase little-endian product code / bounded printable ASCII product name | No | Valid product descriptor; model omitted when empty/malformed, independently of fixed fields. |
| EDID Manufacturer OUI / Model (block {n}, CTA-861) | Product Information block CID/OUI, least-significant byte first / optional ASCII product name | No | CTA revision 3 extended tag 0x21; manufacturer present without version, model only for version 0. Zero/all-ones OUI omitted. |

**Sources/order.** `root\wmi:WmiMonitorID` with no Active filter → registry inventory when query fails or returns zero rows. WMI UInt16 arrays discard all NUL elements; no placeholder/whitespace cleanup. Enrichment maps `InstanceName` (strip trailing `_0`) to exact HKLM `SYSTEM\CurrentControlSet\Enum\DISPLAY\{model}\{instance}\Device Parameters\EDID`; never borrow another same-brand monitor. Fallback enumerates all cached DISPLAY instances in registry order, parses base blocks, then checks shared present IDs. Override detection opens that instance's `Device Parameters\EDID_OVERRIDE`. Preserve WMI values and add only missing/different registry fields; registry is cached data, not fresh EEPROM.

Extension enrichment follows all existing lines in each WMI monitor group. Match the complete `InstanceName` case-insensitively to `root\wmi:WmiMonitorDescriptorMethods`, use its returned object path, and call `WmiGetMonitorRawEEdidV1Block` with a CIM UInt8 `BlockId`. Read driver block 0 for the expected extension count, then blocks 1 through that count (cap 32), ending that monitor's sequence on method failure. Registry fallback remains base-block-only. No extension value replaces or deduplicates a base value; conflicting/duplicate descriptor values retain block/source labels and descriptor order. Driver-exposed bytes are not a guaranteed fresh physical DDC read. Native providers may omit `ReturnValue`; the additive input-aware helper checks it when present and always propagates COM errors. The existing no-input helper, including TPM, is unchanged.

**Extension validation/specification.** Require exactly 128 bytes and a zero modulo-256 EDID checksum. Tag 0x70 is DisplayID: structures 0x10–0x13 and 0x20 only; require its independent section checksum and payload length at most 121. Product tags 0x00 (1.x) and 0x20 (2.0) are version-gated, revision 0, at least 12 payload bytes; validate the declared name length separately. Tag 0x0A is the 1.x serial-text descriptor. Tag 0x02 is CTA-861 revision 3: bound its data-block collection before parsing extended tag 0x21 (PIDB), with optional version 0 and at most 25 name bytes. Other timing, capability and opaque vendor payloads are not interpreted as identity. Text must be printable ASCII after outer trailing whitespace/NUL padding; malformed text contributes only diagnostics. Serials use `Out::id`, so existing masking applies to values of at least four characters, with case-sensitive whole-token matching; shorter values retain the existing masking limitation. Manufacturer, product code and model are context, not per-unit IDs.

References: [Microsoft WMI method](https://learn.microsoft.com/en-us/windows/win32/wmicoreprov/wmigetmonitorraweedidv1block-wmimonitordescriptormethods); [VESA DisplayID v2.1a](https://vesa.app.box.com/s/uf8t0mbo79enb1r4xlxr7zdwvgnoi12n/file/1476779114121), section 2.1 and tables 4-1 through 4-6; CTA-861.7 section 7.5.20/table 122, identified by [VESA's standards reference](https://vesa.org/faqs/); [edid-decode versioned tag/parser reference](https://android.googlesource.com/platform/external/edid-decode/+/9e5323dfc606646b2c5a179562f587c1ac14761d/parse-displayid-block.cpp).

**Admin/timing/limits.** Admin app; registry/driver data. Extension work has one 2-second total section budget, including worker-local COM connection, query and all method calls. The receiver retains completed blocks and base fields on timeout; failures (absent, unsupported, denied, malformed and timeout) go only to diagnostics. A synchronous WMI call may outlive the budget; it is not joined, and an active-worker guard prevents accumulating abandoned workers across refreshes. No further call starts after the deadline. Base-block behavior remains: invalid header rejects a group; bad checksum retains identity with diagnostics. Reserved dates omitted. Historic disconnected EDIDs remain in fallback. Invalid WMI detail gives `Error: Error reading monitor details: {E}`; registry scan errors trail valid groups as `Error: {E}`, optional missing keys normally omitted. No valid inventory: `No monitors detected. Please ensure your display drivers are properly installed.`

**2026-10-04 elevated verification.** Supplied baseline median 4 ms → final candidate median 71 ms (five samples via `--time`); final monitor-only dump 52 ms (earlier cold run 99 ms, median 73 ms). Three monitors each returned two extensions, tags 0x02 and 0x70. DisplayID structure 0x12 timing blocks supplied no supported identity fields; one had a valid outer checksum but invalid inner checksum, correctly diagnostic-only. Monitor section (482 bytes including terminal CRLF) and TPM section (315 bytes) were byte-identical to baseline; monitor-only/full monitor bodies also matched. Captures stay in session temp `opencode/wp-a4`. Final `check.ps1` passed formatting, clippy, 133 enabled tests, release build and resource versions, then stopped at the shared-build import allow-list (`propsys.dll`, outside WP-A4); the manifest step did not run. **Untested on hardware:** identity-bearing DisplayID 1.x product/serial blocks, DisplayID 2.0 products and CTA PIDB; absent/unsupported/access-denied/timeout, 32-block cap and registry-fallback paths. Fabricated parser fixtures cover checksum, framing, version/tag/revision, endian and text validation; a fabricated rendering check confirms serial masking.

**AD:** 01–03, 17–19, 46–48, 81, 82, 105–109. **Not built:** DisplayConfig topology or vendor DDC/I²C read — path correlation or closer device data; driver/dock/KVM compatibility cost, untested. DisplayConfig target names do not supply raw EDID serials.

## NETWORK ADAPTERS (NIC's)

[Provider](src/hw/network.rs), [IP Helper](src/win/iphlp.rs), [NDIS OID](src/win/ndis.rs).

```text
Name: Intel(R) Ethernet Controller I225-V
Product Name: Intel(R) Ethernet Controller I225-V
Device ID: 7
Adapter Type: Ethernet
Hardware ID: PCI\VEN_8086&DEV_15F3&SUBSYS_87D21043&REV_03
MAC Address (Overridden): 02:7C:39:61:B4:8E
Permanent MAC: 3C:FD:FE:72:19:A6
Permanent MAC (OID): 3C:FD:FE:72:19:A6
```

| Label | Meaning | ID | Appears when |
|---|---|---|---|
| Name / Product Name | WMI names | No | Every accepted adapter. |
| Device ID | WMI DeviceID, not interface index | No | Every accepted adapter. |
| Adapter Type | Simplified WiFi/Ethernet/Bluetooth/uppercase kind | No | Every accepted adapter. |
| Hardware ID | First SetupAPI hardware ID | Yes | Cached PNP match exists. |
| MAC Address / MAC Address (Overridden) | WMI current address | Yes | Every accepted adapter, even empty. |
| Permanent MAC | Driver-reported permanent address or `Unavailable` | Yes for address; no for unavailable | Every accepted adapter. |
| Permanent MAC (OID) | Six-byte driver-reported `OID_802_3_PERMANENT_ADDRESS`, uppercase colon-separated | Yes | A uniquely matched present miniport interface opens and returns exactly six bytes. Immediately after Permanent MAC; absent on OID failure. |

**Sources/order.** Inventory/names/type/DeviceID/current MAC: `root\cimv2:Win32_NetworkAdapter`; hardware ID: shared SetupAPI by PNPDeviceID. Permanent MAC: `GetIfTable2.PermanentPhysicalAddress`, matched uniquely to WMI GUID, nonzero and ≤32 bytes. If permanent known, case-insensitive current/permanent inequality selects Overridden (empty current does not). Otherwise registry `NetworkAddress` presence determines that label: HKLM `SYSTEM\CurrentControlSet\Control\Class\{4d36e972-e325-11ce-bfc1-08002be10318}\{numeric subkey}`, using DeviceInstanceID exact or MatchingDeviceId prefix matching. Effective comparison takes precedence over stale registry config. Native-only replacement was rejected because it did not preserve all WMI fields with useful speed gain.

**OID corroboration/source priority.** SetupAPI enumerates present `GUID_NDIS_LAN_CLASS` miniport interfaces and obtains each interface's associated device instance ID. Exact case-insensitive PNPDeviceID matching selects accepted WMI adapters; multiple matching interfaces suppress that adapter's OID line with an ambiguity diagnostic. Read-only `IOCTL_NDIS_QUERY_GLOBAL_STATS` requests only `OID_802_3_PERMANENT_ADDRESS`. Its value is independent of the existing GetIfTable2 line and never selects the Overridden label or replaces an unavailable GetIfTable2 value. Report the returned bytes even if zero, locally administered, or different from GetIfTable2. OID agreement is corroboration from the miniport, not independent EEPROM/factory authenticity. The IOCTL is deprecated and Wi-Fi behavior can differ. Absent interface, unsupported IOCTL/OID, access denial, malformed response, enumeration/open failure, and timeout remain diagnostics only; Win32 error codes are retained without device identifiers. Existing WMI, GetIfTable2, registry, and hardware-ID results survive every OID failure.

**Admin/timing/limits.** Admin app; WMI/driver/registry. Elevated WP-A5 measurement on 2026-10-04: section median 118 ms baseline → 119 ms final (initial after run: 126 ms), Mellanox ConnectX-3 Ethernet Adapter; OID and GetIfTable2 agree. Existing section bytes match after removing the one OID line. The OID scan waits at most 750 ms total and retains completed results; synchronous calls cannot be forcibly stopped, so a worker owns its buffers/handles until return and a busy guard caps outstanding scans at one. Enumeration is capped at 1024 interfaces and interface-detail allocation at 64 KiB. Wi-Fi, other NIC vendors, multiple NICs, disagreement, absent/unsupported/denied/malformed responses, hotplug, timeout and repeated-collection busy paths remain untested on hardware. Requires non-null MAC, PhysicalAdapter not false, PCI/USB-like bus (or MLX4/MLX5), physical Ethernet/WiFi type (or Mellanox), and no virtual/VPN/TAP/TUN/bridge/security-client keyword in name/product. This is a heuristic, not complete physical inventory; empty accepted set leaves a blank body. Permanent address remains a miniport report, not EEPROM proof. Registry fallback can match stale/prefix entries. Failures can add `MAC override lookup: {E}` or `Hardware ID lookup: {E}`.

Excluded name/product substrings (case-insensitive): VIRTUAL, VPN, TAP, TUN, TUNNEL, VMWARE, HYPER-V, VIRTUALBOX, CISCO, CHECKPOINT, FORTINET, JUNIPER, CITRIX, SOFTETHER, OPENVPN, WIREGUARD, GHOST, HAMACHI, NDIS, BRIDGE, LOOPBACK. Bus accepts PNP prefixes `PCI\`, `USB\`, `MLX4\`, `MLX5\` or embedded `PCI_`/`USB_`.

**AD:** 01–03, 20, 46, 94. **Not built:** Vendor NIC NVM/EEPROM or PCIe DSN — possible factory inventory; vendor-specific access/optional capability and driver research cost, untested.

## BLUETOOTH ADAPTERS

[Provider](src/hw/bluetooth.rs), [radio helper](src/win/bluetooth.rs).

```text
Adapter: WORKSTATION-K731
MAC Address: 3C:FD:FE:85:27:B1
```

| Label | Meaning | ID | Appears when |
|---|---|---|---|
| Adapter | Native radio `szName`, WMI device name, or generic fallback | No | Each selected radio/adapter. |
| MAC Address | Local radio address / legacy registry address | Yes for address | Each adapter; literal `MAC not available` is unmarked. |

**Sources/order.** Runtime-loaded `BluetoothApis.dll`: `BluetoothFindFirstRadio/NextRadio` + per-handle `BluetoothGetRadioInfo`. Any successful radios print their own names/addresses and unresolved radio errors. With no usable radios: all-device SetupAPI precheck rules out impossible WMI scans → `root\cimv2:Win32_PnPEntity` where PNPDeviceID LIKE `USB%` and Name LIKE `%Bluetooth%`, with legacy registry address → registry alone → WMI Service=`BTHUSB` without address. Registry: HKLM `SYSTEM\CurrentControlSet\Services\BTHPORT\Parameters\Bluetooth Host Controller\LocalRadioAddress`. Native six-byte address reversed to uppercase colon notation; fallback reverses the entire registry value when length ≥6. Native per-radio pairing avoids copying one global address to multiple radios; legacy fallback retains that old behavior.

**Admin/timing/limits.** Admin app; local radio APIs. Latest ~52 ms on no-radio hardware. **F-06a open:** native `szName` may be the PC/radio name, not adapter model; successful radio naming/address path unverified on a Bluetooth-equipped laptop. Fallback can include historical devices and reuse one global address. No adapters gives `No Bluetooth adapters detected.` Unresolved errors can appear as `Error: {E}`.

**AD:** 01–03, 08, 46. **Not built:** exact radio-interface SetupAPI friendly-name correlation — adapter model instead of possible host name; hardware association and laptop evidence cost. Extra controller metadata — vendor/revision context; API/presentation validation cost. Remote BLE devices are outside this local-adapter section.

## AUDIO DEVICES

[Provider](src/hw/audio.rs), [Core Audio helper](src/win/audio.rs).

```text
Adapter: USB Audio Device
Instance ID: USB\VID_046D&PID_0A9F\A7C28E41
Hardware IDs: USB\VID_046D&PID_0A9F&REV_0100
Container ID: {52B18C39-7D64-4AF0-963E-826A19DB4507}
Endpoint: Microphone (USB Audio Device)
Direction: Capture
Endpoint ID: {0.0.1.00000000}.{941d59a2-72ce-4536-a71a-66af60dd2788}
Endpoint: Speakers (USB Audio Device)
Direction: Render
Endpoint ID: {0.0.0.00000000}.{6d73e082-684f-4130-a0cb-fb538d1c3279}
```

| Label | Meaning | ID | Appears when |
|---|---|---|---|
| Adapter | `PKEY_DeviceInterface_FriendlyName`; `Unknown Audio Adapter` if absent; final unresolved group is `Unresolved Audio Adapters` | No | Once per adapter group. |
| Instance ID | Adapter devnode: endpoint `PKEY_Device_InstanceId` first, then topology adapter's `PKEY_Device_InstanceId` | Yes | Either route resolves the adapter; preserve returned case. Endpoint `SWD\MMDEVAPI\` IDs are not adapter grouping keys. |
| Hardware IDs | Shared SetupAPI hardware-ID map joined by uppercase instance ID | Yes | Snapshot has a matching hardware ID. |
| Container ID | A3 SetupAPI helper, exact present devnode's braced uppercase container GUID | Yes | Present devnode/property exists and GUID is non-null. |
| Endpoint Container ID | A3 helper on `SWD\MMDEVAPI\{opaque endpoint ID}`, labelled at that endpoint | Yes | Adapter Container ID is unavailable and the endpoint container exists. Never presented as an adapter container. |
| Endpoint | `PKEY_Device_FriendlyName`; `Unknown Audio Endpoint` if absent | No | Each active endpoint. |
| Direction | `Render` or `Capture` | No | Each active endpoint. |
| Endpoint ID | Opaque `IMMDevice::GetId` string, unchanged | Yes | GetId succeeds with a nonempty string; includes device-specific GUID identity. |
| Stable ID | `PKEY_AudioEndpoint_StableId`: opaque case-sensitive string unchanged, or braced GUID from `VT_CLSID` | Yes | Windows 11 24H2+ supplies a nonempty string or non-null GUID; optional even on supported Windows. |

**Sources/order.** Worker-local COM MTA (retain an existing STA as in WMI), `MMDeviceEnumerator` → active render/capture endpoints, read-only `STGM_READ` property stores. GetId and each property are independent: one failure preserves the other successful fields. Adapter instance ID prefers the endpoint property; absent/failed/software-endpoint values use `IMMDevice::Activate(IDeviceTopology)` → `GetConnector(0)` → [GetDeviceIdConnectedTo](https://learn.microsoft.com/en-us/windows/win32/api/devicetopology/nf-devicetopology-iconnector-getdeviceidconnectedto) → `IMMDeviceEnumerator::GetDevice` → the adapter's read-only property store and `PKEY_Device_InstanceId`. The topology token is an opaque MMDevice ID, which can have a `{2}.` prefix; do not pass it as a SetupAPI interface path or infer a devnode by parsing it or joining a name. The CfgMgr32 interface-property/direct SetupAPI-interface routes failed on this PC and are not used. Group by case-insensitive adapter instance ID, ordered by uppercase instance ID, then direction (`Capture` before `Render`), endpoint name, and opaque endpoint ID as a tie-breaker. All unresolved endpoints share one final group in the same endpoint order. Print adapter fields once, then each endpoint's fields; 40-dash separators divide groups. Shared SetupAPI hardware-ID snapshot and a present-all `DevInfoSet` supply independent hardware/container joins. Missing adapter container falls back independently to each endpoint's software devnode, with the distinct `Endpoint Container ID` label. No WMI fallback, audio-stream activation, or device changes. Read `VT_LPWSTR`/`VT_CLSID` directly and release PROPVARIANTs with OLE32 `PropVariantClear`; no propsys conversion helpers. All six identity labels use `Out::id`; complete tokens retain case and satisfy the existing minimum-four-character, whole-token masking rules.

**Admin/timing/limits.** Admin app; synchronous COM calls run under the collector's existing 60-second provider deadline. Endpoint/property strings are limited to 32767 UTF-16 units and reject malformed UTF-16. `VT_EMPTY`, `VT_NULL`, empty strings and null GUIDs omit optional fields with diagnostics; unsupported types, access-denied, timeout, and other failures retain separate diagnostics and do not remove successful endpoints. Inactive (disabled, not-present, unplugged) endpoint counts appear only in diagnostics, separately for render/capture. Empty successful active enumeration gives `No active audio endpoints detected.` Incomplete empty enumeration reports an error instead. Endpoint IDs and stable IDs are OS/driver identities, not guaranteed immutable physical serial numbers; virtual devices can appear. Hardware-ID/ContainerID joins can be absent independently. Elevated owner-PC round 2 capture on 2026-10-04: 25 active endpoints (15 render, 10 capture), grouped as BEACN Studio (22), NVIDIA Broadcast (2), and HyperX Cloud II Wireless (1); all three adapters supply instance, hardware and container IDs. Endpoint property InstanceId is absent, so every adapter uses the topology fallback. 24 inactive render and one inactive capture endpoint are excluded. Section-only dump 13 ms; five-run median 16 → 11 ms from round 1, not a general speed claim. The real `report::masked` function masked all 34 captured identity values; fabricated-fixture verification covers optional Stable ID. Earlier development captures exercised endpoint-container reads and one final unresolved group. **Untested:** `PKEY_AudioEndpoint_StableId` (absent on every endpoint on this Windows build), successful endpoint-property-first adapter join, naturally unavailable adapter/container, no-endpoint hardware, older Windows, access-denied, COM/enumeration/property failures, malformed UTF-16/property types, and provider timeout. Captures remain in session temp.

**AD:** 01–03, 46, 116. The C# app has no audio section; `check.ps1` lists it in `$RustOnlyTitles`.

## ARP INFO/CACHE

[Provider](src/hw/arp.rs), [IP Helper](src/win/iphlp.rs).

```text
[Ethernet]
MAC: 3C:FD:FE:69:42:B8 | IP: 192.0.2.31
MAC: 00:1B:21:73:95:C4 | IPv6: 2001:db8::31

[vEthernet (WSL)] (Virtual)
MAC: 02:41:67:93:A8:2C | IP: 198.51.100.26
```

| Line/label | Meaning | ID | Appears when |
|---|---|---|---|
| `[name]` / `[Interface #n]` | Interface friendly name or numeric fallback | No | Each native group; ascending interface index. |
| `(Virtual)` | Name-keyword classification | No | vEthernet/Loopback/WSL/Docker/Hyper-V/VPN/VMware/VirtualBox match. |
| Dynamic ARP Entries: | Ungrouped fallback heading | No | `arp.exe` has at least one accepted row. |
| MAC | Neighbor-cache link-layer address | Yes | Relevant native row or accepted `arp.exe` row. |
| IP / IPv6 | Neighbor address, IPv6 without scope ID | Yes | With MAC; IPv4 before IPv6, then textual address sort on native path. |
| Status | `No relevant dynamic ARP entries found.` | No | No accepted rows; text is legacy, native path also retains permanent/unreachable states. |

**Sources/order.** `GetIpNetTable2(AF_UNSPEC)` → absolute System32 `arp.exe -a` only on API failure, not empty success. `GetAdaptersAddresses` maps both IPv4/IPv6 interface indices to names; failure retains numbered groups plus `Interface name lookup: {E}`. Native filtering drops only Incomplete state, empty/all-zero physical address and multicast/broadcast MACs (low bit of first byte). `arp.exe` parser instead accepts English, case-sensitive `dynamic` lines and excludes interface headers, broadcast/IPv4 multicast and `static` lines; it does not group/sort or provide equivalent IPv6 coverage. Both are OS cache sources; no firmware replacement exists.

**Admin/timing/limits.** Admin app; cache reads. Latest logged native 2–3 ms, fallback ~43 ms; process capped at 10 s. Volatile peer entries are not local NIC permanent addresses. Localized arp output is unverified. Final failure: `Error: Unable to retrieve ARP information: {E}`. The body is trimmed at end.

**AD:** 01–03, 21, 46; OPT-9 keeps native permanent/unreachable states. **Not built:** explicit neighbor state/source labels — explains stale/permanent entries; small formatting/AD cost. Active peer discovery would change the question and create traffic; outside current collection.

## Adding a new identifier

1. Read this reference and the relevant `src/hw/{section}.rs` (disk uses `src/hw/disk/`). Put OS calls and bounded parsers in `src/win/`; reuse `Ctx` and existing sources where appropriate. Preserve public signatures and current inventory/order.
2. State what the value identifies and how it associates to the exact device. Keep different layers/sources under different labels. Treat absence, unsupported, access denied, timeout and malformed data separately; keep fallback diagnostics.
3. Mark values with `Out::id(label, value)`, `combined`'s `true`, or `id_value(value)` for free-layout text. Verify Mask IDs and JSON `ids` on the real resulting output; account for the four-character/token-boundary rules.
4. Capture **elevated before/after** reports and diagnostics privately with the same hardware/build conditions. Byte-diff section bodies, IDs, groups, padding and order; identify volatile cache lines separately. Measure section timing with the existing timing mode, including fallback/failure paths. Use fabricated fixtures only where real checks cannot reliably prove parser/security boundaries.
5. Propose an AD row for each intentional visible difference; the orchestrator owns approval. Keep unchanged fields byte-identical. Verify relevant real paths plus the normal code-change gates (`check.ps1`); name untested hardware/fallback coverage.
6. Update this file in the same commit: shown lines, marking, source priority, conditions, limits and measured timing. Every public example must be independently fabricated with realistic vendor formats; never copy real capture values.

## Not collected (by design or deferred scope)

- **Non-admin compatibility paths:** outside the administrator-only product contract; no handle tweaks/fallbacks solely for non-admin collection.
- **Arbitrary kernel/physical memory or register reads:** no collection driver is shipped. Source-first user-mode reads remain preferred; research a narrow signed driver only for a concrete source inaccessible from user mode.
- **Raw SPD, PPIN, PCIe DSN, NIC EEPROM/NVM:** deferred, hardware-specific privileged interfaces and lifecycle/security costs; no generic universal manufacturing-ID guarantee.
- **TPM private keys, provisioning/clearing, attestation and certificate-chain/key-binding validation:** this collection reports status/public EK metadata; it does not prove possession or trust. Native helper does not create a key.
- **Remote Bluetooth/BLE device inventory or active network probing:** scope is local adapters and existing neighbor-cache state.
- **Arbitrary UEFI variables, firmware-memory dumps in the normal report, EDID extensions and raw live DDC reads:** deferred or separate diagnostics/research; current firmware/EDID collection is explicitly scoped above. `--dump-raw-smbios` is a separate read-only diagnostic command, not a shown section field.
- **Universal CPU/GPU manufacturing serial:** available source fields are reported with their own semantics. ProcessorId/CPUID signature, GPU PNP IDs and DXGI LUID do not establish one.
