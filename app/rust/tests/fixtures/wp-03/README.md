# WP-03 fixtures

The historical WP-03 capture on 2026-10-03 returned error 1168 for MSDM;
the WMI licensing fallback and registry ProductId matched the C# WQL reference.
That capture remains private; the tracked parser input here is `msdm.hex`.

`msdm.hex` is a synthetic 85-byte ACPI MSDM table with a fabricated 29-character
product key and OEM metadata. Its ACPI checksum is repaired. It contains no real
hardware identifiers or usable Windows licence.

The fixture follows PLAN WP-03's key offset (56) and length (29), and the ACPI
header plus MSDM data type/length fields. Tests also mutate and truncate it to
check bounds, header/length validation, checksum errors, and non-ASCII/control
bytes. This is a parser fixture, not evidence of parity on a real MSDM-equipped
machine; live captures remain private under the main checkout's `golden/wp-03/`.
