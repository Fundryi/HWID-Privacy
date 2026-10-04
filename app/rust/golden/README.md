# Private golden captures

This folder holds local C# baselines, Rust reports, diagnostics, timings, and
comparison evidence for the Rust port. Everything except this README is
git-ignored. Real identifiers never leave this folder: do not commit, upload,
or paste its captures into an agent report or public issue. Committed fixtures
under `../tests/fixtures/` must contain fabricated identifiers only.

From PowerShell, run `../check.ps1` for the checks that need no elevation.
Hardware modes require an **elevated PowerShell**:

```powershell
./app/rust/check.ps1 -Golden
./app/rust/check.ps1 -Golden -Sections 'USB DEVICES', 'CPU'
./app/rust/check.ps1 -Ghosts
./app/rust/check.ps1 -Logs
./app/rust/check.ps1 -Timing
```

Run those commands from the repository root. They build GoldenDump from
`app/tools/GoldenDump/` and only invoke read-only switches. They never clean
devices or logs, publish, or copy an executable to the repository root.

Each hardware run saves its files under `owner-pc/<timestamp>-<unique-id>/`
in the main checkout's golden folder (located via Git's common directory).
Golden runs capture C# twice to expose volatile values and record the source
commit and executable hashes. Full reports require all 14 sections in order.
Selected Rust sections must be implemented. Comparisons preserve UTF-8 BOMs,
CRLF, padding and all other bytes; only USB device groups are sorted by ordinal
order before comparison.

Golden differences print the section, line number and JSON-escaped line
(including whitespace), marked **needs approval**, and fail the check. Consult
the main checkout's `docs/rust-port/approved-diffs.md`; the script does not
approve or automatically match its rules. The orchestrator reviews every
difference, including volatile lines. Ghost comparisons ignore the InstanceId
and C# legacy Presence columns and enforce both membership rules from WP-11.
Logs compare in their emitted order. Timing is informational and flags Rust
medians more than 20 percent slower than C#.
