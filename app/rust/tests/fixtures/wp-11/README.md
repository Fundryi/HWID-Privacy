# WP-11 whitelist fixtures

All identifiers are fabricated. The USB vendor/product-shaped strings contain no captured device serial or instance ID.

- `csharp-whitelist.json`: exact output of the unchanged legacy `DeviceDetailConverter` for the fabricated Rust fixture, apart from JSON line endings/trailing newline. Includes HTML-sensitive escapes, an umlaut, a surrogate pair, joined hardware IDs, and an empty ID.
- `rust-whitelist.json`: the `serde_json` pretty output asserted by the Rust test, apart from the final fixture newline. The written file uses CRLF like C# `WriteIndented` on Windows; the test normalizes the fixture's own line endings, which follow git autocrlf. The field order is Name, Description, HardwareId, Class.

The C# fixture records the historical [converter format](https://github.com/Fundryi/HWID-Privacy/blob/3768ddc9c21c9e64f8ada067d8466c7e9f7460e3/app/src/Services/DeviceWhitelistService.cs). Rust tests retain compatibility with whitelists saved by installed legacy clients; regenerating a C# oracle requires a separate historical checkout.

Run from `app/rust`:

```powershell
cargo test --locked --lib clean::
```

The ignored `wp11_read_only_scan` test prints real device data. Its output belongs only in the private `D:/GIT/HWID-Privacy/app/rust/golden/wp-11/` folder, never here.
