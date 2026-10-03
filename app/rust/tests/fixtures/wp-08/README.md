# WP-08 monitor fixtures

`monitors.json` contains EDID base blocks derived from the read-only owner-PC
capture on 2026-10-03. Manufacturer/product IDs, numeric serials, manufacture
week/year, descriptor serials, model names, and all instance IDs were replaced with fabricated values.
All original descriptors were removed before inserting the fabricated model and
serial descriptors. Extensions were removed, the extension count set to zero,
and each base-block checksum repaired. Raw captures stay in the ignored
`D:/GIT/HWID-Privacy/app/rust/golden/wp-08/` folder.

The set contains two connected monitors of the same brand/model, a historical
same-brand monitor, and a WMI instance with no registry match. The parser tests
also exercise the listed placeholder serials and truncated lengths, a corrupt
header, a corrupt checksum with retained data, descriptor ordering/ASCII quirks,
and WMI UTF-16 strings with interior zeros. Registry presence appears after all
legacy fields and only when a successful SetupAPI present snapshot excludes the
instance. A failed snapshot never asserts disconnection.
