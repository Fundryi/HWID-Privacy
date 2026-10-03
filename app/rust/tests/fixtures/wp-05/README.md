# WP-05 parser fixtures

All identifiers and certificate names here are fabricated. These are synthetic
Format-List parser cases derived from `app/src/Hardware/TpmInfo.cs:240-272`, not
a claim of an elevated hardware capture or native EK verification. The input
exercises multiple certificates, case-insensitive section names, ignored wrapped
lines, and the legacy last-certificate-wins behavior. The expected file uses LF;
the test converts it to the required CRLF before a byte-for-byte comparison.

The real, non-elevated provider capture is opt-in:

```powershell
cargo test --locked --lib -- --ignored wp05_capture_tpm --nocapture
```

It prints real values and saves them only under the git-ignored absolute path
`D:\GIT\HWID-Privacy\app\rust\golden\wp-05\`. Do not copy those values here.
