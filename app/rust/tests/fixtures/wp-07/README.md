# WP-07 GPU fixtures

Every UUID, PCI instance suffix, hardware-ID subsystem value, and board serial in this folder is fabricated. No owner-PC identifiers are stored here. The `.fixture` extension avoids the repository's `*.txt` ignore rule without changing shared files.

- `nvidia-smi.fixture`: the `-L` line format verified against the installed driver, with replacement UUIDs. The second GPU and its ignored MIG line are synthetic parser coverage. The parser is checked with both LF and CRLF input.
- `mixed-gpus.fixture`: approved single-NVIDIA output with a fabricated ASCII board serial and an additional Intel adapter (AD-12, AD-14). Its tree and blank lines follow `Hardware/GpuInfo.cs:52-75,103-120`.
- Unit-test byte arrays: fabricated printable/NUL board bytes, zero placeholders, and binary bytes. Binary board values use uppercase hexadecimal pairs with **no separators**, including all sixteen bytes (AD-13).

Raw native output, nvidia-smi cross-checks, WMI-only text, and NVAPI bytes remain in `D:/GIT/HWID-Privacy/app/rust/golden/wp-07/`.
