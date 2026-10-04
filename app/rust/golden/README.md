# Private captures

Keep Rust reports, diagnostics, timings, and manual verification evidence here.
Everything except this README is git-ignored. Do not commit, upload, or paste
real identifiers into reports or public issues. Committed data under
`../tests/fixtures/` must contain fabricated identifiers only.

Run `pwsh -NoProfile -File app/rust/scripts/check.ps1` from the repository root
for formatting, lint, tests, and executable checks. It needs no hardware capture.

For read-only hardware inspection, run the built executable as administrator
with `--dump <private-file>`, `--ghosts <private-file>`, `--logs <private-file>`,
or `--time <private-file>`. Use an absolute output path under this ignored
folder and a new subfolder for each run. Inspect `--dump` diagnostics too.
Never use cleaning or update installation as part of a read-only capture.

Historical C# parity captures remain private. Reproducing them requires a
separate historical checkout and its matching harness; the current gate
verifies the Rust implementation and its fabricated fixtures.
