# WP-02 SMBIOS fixture

`smbios.hex` is a whitespace-delimited hexadecimal RawSMBIOSData buffer.
Its Type 0, 1, 2 and 3 layouts came from a read-only, non-elevated firmware
capture on 2026-10-03. The original capture stays in the private
`D:/GIT/HWID-Privacy/app/rust/golden/wp-02/raw-smbios.bin`.

All strings were replaced with fabricated vendor-consistent examples,
including serials, asset tags and SKU. The system UUID and structure
handles were replaced; the chassis OEM-defined value was cleared.
The RawSMBIOSData length was recalculated, and a Type 127 end marker was
added. This wrapper has no checksum. Other firmware structure types were
excluded so their identifiers cannot enter the fixture.

Provider tests use the shared SMBIOS parser and exercise exact CRLF text,
optional fields, last-present-field overwrite behavior, manufacturer gates,
UUID byte order on old and new SMBIOS versions, sentinel UUIDs, and BIOS
enrichment failures. Test mutations represent repeated and short records.
