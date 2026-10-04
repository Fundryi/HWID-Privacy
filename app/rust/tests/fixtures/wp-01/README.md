# WP-01 disk fixtures

All device-specific values here are fabricated. These are synthetic SDK-layout
fixtures, not sanitized captures from this PC: at fixture creation, the worker shell could not open
physical drives with the historical C# GENERIC_READ access. Hardware-derived descriptor and
layout fixtures still require an elevated owner capture and identifier replacement.
Real runtime data is confined to the ignored main-checkout golden/wp-01 directory.

- storage.json: native STORAGE_DEVICE_ID_DESCRIPTOR / STORAGE_IDENTIFIER buffers;
  ASCII priority, association ignored for C# parity, binary priority, first fallback,
  zero count, zero-length ASCII, printable and non-printable 16-byte identifiers,
  ASCII replacement/NUL handling, size/count/next-offset overruns, chain stop at
  count or a zero NextOffset (C# keeps the best identifier so far). The case
  observed-shape-* copies the header fields of a real NVMe descriptor (count 1,
  UTF-8 SCSI name string, last NextOffset aligned up to Size); its payload is
  fabricated. GPT, MBR, RAW,
  truncated/count-overflow layouts; single-disk, repeated-disk and spanned extents.
- unique-ids.json: all five Guid.TryParse formats, case/whitespace handling,
  mixed-endian ToByteArray, D/X compatibility prefixes and overflow boundaries,
  original-string UTF-16-to-byte fallback, singleton/array PowerShell JSON.
  The 21 conversion expectations were independently checked with .NET 10.0.11.
- disk-tree.json: fabricated two-disk output, multiple volume letters, absent values,
  identifier registration, GPT details and error-only lines. Its escaped CRLF and
  trailing spaces are compared literally without report normalization.

Binary layouts are expressed as hexadecimal strings. Tests derive SDK offsets
with offset_of! and assert the known native header/entry sizes independently.
Every truncated prefix of a valid descriptor or GPT/MBR layout must be rejected.

The ignored hw::disk::tests::wp01_capture_nonadmin test runs collect_provider,
writes a private report and diagnostics, and compares native volume mappings and
serials against WMI associations and Storage UniqueIds against PowerShell.
It prints real identifiers: capture its console output privately if running it.
Its source-comparison file records mismatches; inspect that file as well as the
test status. The test never invokes HWIDChecker.exe or destructive operations.
