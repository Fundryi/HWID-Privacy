# WP-11 whitelist fixtures

All identifiers are fabricated. The USB vendor/product-shaped strings contain no captured device serial or instance ID.

- `csharp-whitelist.json`: exact output of the unchanged legacy `DeviceDetailConverter` for the fabricated Rust fixture, apart from JSON line endings/trailing newline. Includes HTML-sensitive escapes, an umlaut, a surrogate pair, joined hardware IDs, and an empty ID.
- `rust-whitelist.json`: the `serde_json` pretty output asserted by the Rust test, apart from the final fixture newline. The written file uses CRLF like C# `WriteIndented` on Windows; the test normalizes the fixture's own line endings, which follow git autocrlf. The field order is Name, Description, HardwareId, Class.
- `verify-whitelist.ps1`: compiles the existing C# converter/model in memory, reads the Rust fixture, and verifies the C# serialization oracle. It replaces only the unused native SP_DEVINFO_DATA type with an empty test struct. It does not launch either application, call SetupAPI, save/reset the whitelist, or edit C# sources.

Run from `app/rust`:

```powershell
rtk proxy cargo test --locked --lib clean::
rtk proxy pwsh -NoProfile -File tests/fixtures/wp-11/verify-whitelist.ps1
```

The ignored `wp11_read_only_scan` test prints real device data. Its output belongs only in the private `D:/GIT/HWID-Privacy/app/rust/golden/wp-11/` folder, never here.
