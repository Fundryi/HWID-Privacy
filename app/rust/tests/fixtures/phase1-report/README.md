# Phase 1b-5 formatter fixtures and implementation report

Worktree: `D:/GIT/HWID-Privacy-wt/p1b-5`, branch `rust/p1b-5`, base `19f9f2a`.

## Files changed

- `app/rust/src/report.rs`: formatter parity comments, the main-window export format, non-expanding ordinal case matching, and inline formatting/builder tests.
- `app/rust/src/hw/mod.rs`: parallel provider collection with deadlines, panic/error diagnostics, fallback chains, cache documentation, and inline collection/report tests.
- `app/rust/tests/fixtures/phase1-report/formatter-cases.json`: 17 fabricated C# formatter calls and their literal expected output. The extra `expected` property is ignored by GoldenDump.
- `app/rust/tests/fixtures/phase1-report/README.md`: fixture provenance and this report.

No existing public signatures changed. No public items were added. No new crates, production `unwrap()`/`expect()` calls, or unsafe blocks were added. The 14 provider files and C# source files were not edited. No commits or application launches were performed.

## Public functions

| Function | Behavior |
|---|---|
| `Out::new` | Creates an empty body with identifier, source, and failure records. |
| `Out::info` | Appends a labeled value and CRLF. |
| `Out::id` | Records the identifier and appends its labeled value. |
| `Out::combined` | Appends labels/values separated by ` | ` and records values whose identifier flag is true. |
| `Out::separator` | Appends the 40-character item separator and CRLF. |
| `Out::blank` | Appends CRLF. |
| `Out::text` | Appends free-layout text followed by CRLF, preserving the supplied text. |
| `Out::id_value` | Records an identifier embedded in free-layout text without changing the body. |
| `Out::source` | Records the successful source without changing visible text. |
| `Out::fallback_failed` | Records the source and error without changing visible text. |
| `Out::finish` | Returns the body and its metadata as a `Section`. |
| `separator` | Returns 93 equals signs followed by CRLF. |
| `centered` | Applies the C# left-padding rule using UTF-16 width, then appends CRLF. Long text is preserved. |
| `format_header` | Wraps centered text between two main separator lines. |
| `format_section` | Adds the legacy heading, preserves the raw body, and adds CRLF only when it is missing. |
| `info_line` | Returns `label: value` followed by CRLF. |
| `combined_line` | Returns labeled values joined by ` | ` and followed by CRLF; an empty list produces CRLF. |
| `item_separator` | Returns 40 hyphens followed by CRLF. |
| `device_group` | Renders device fields with item separators between groups, including empty groups. |
| `section_content` | Trims .NET-compatible Unicode whitespace; an empty result becomes `No data available`. |
| `export_text` | Renders supplied sections in order as `===== TITLE =====`, trimmed/display body, and a blank line. Loading placeholders are preserved. |
| `pad_right_utf16` | Pads to the requested UTF-16 width without truncating. |
| `trim_net` | Returns the borrowed string after Unicode whitespace trimming. |
| `eq_ignore_case` | Compares labels using non-expanding ordinal casing, including supplementary characters. |
| `contains_ignore_case` | Searches at Unicode character boundaries using the same non-expanding casing. |
| `Ctx::new` | Creates fresh per-collection OnceLock caches. |
| `Ctx::hardware_id` | Looks up an uppercase-normalized instance ID; a missing ID or failed snapshot returns `None`. The cached error remains available through `hardware_ids`. |
| `Ctx::smbios` | Returns the cached firmware table on success; a cached error remains available through `smbios_result`. |
| `Ctx::hardware_ids` | Initializes the shared SetupAPI map once and returns the map or cached error. |
| `Ctx::smbios_result` | Initializes the shared SMBIOS table once and returns the table or cached error. |
| `Ctx::present_instance_ids` | Initializes the shared normalized present-ID set once and returns the set or cached error. |
| `collect_all` | Filters titles case-insensitively, starts each selected provider on its own detached thread sharing an `Arc<Ctx>`, catches provider panics, and applies a 60-second deadline measured separately from each spawn. Calls `on_done` once per section with its original provider index, then returns sections in provider order. |
| `collect_provider` | Runs one provider inside `catch_unwind`, records its elapsed time, retains partial output on errors/panics, and appends the standard provider error line. Used by sequential timing as well as collection workers. |
| `full_report` | Adds `Comprehensive HWID Checker` and the ordered legacy section headings with untrimmed raw bodies, matching Old View. Empty raw bodies remain blank. |
| `first_ok` | Tries sources in order, records every failure, sets the successful source, and returns the first success or final error. An empty chain returns an error identifying the operation. |

Timed-out workers retain their context and may finish later. Their results cannot replace a timeout or trigger another callback. Spawn/channel failures produce error sections; a result delivery attempted after the collection has ended is recorded on stderr. Timeout bodies are exactly `Error retrieving {title} information: timed out` followed by CRLF, with a diagnostic failure record.

## Tests and checks

Final commands in `app/rust`, all exit 0:

```text
cargo fmt --check
cargo clippy --locked --all-targets -- -D warnings
  cargo clippy: No issues found
cargo test --locked
  cargo test: 18 passed (2 suites, 0.04s)
cargo build --release --locked
  Finished release profile [optimized]
git diff --check
```

`check.ps1` is absent in this worktree. Tests ran without elevation. There are no added ignored tests, and no tests were removed. The release executable was built but never executed. There was no `dotnet publish`.

Added inline tests in `report.rs`:

- `formatter_header_literals_and_utf16_centering`
- `formatter_section_preserves_body_and_only_adds_missing_crlf`
- `formatter_info_combined_and_device_group_literals`
- `section_content_and_export_literals`
- `utf16_padding_literals`
- `out_keeps_identifiers_and_diagnostics_out_of_visible_text`
- `ordinal_matching_does_not_expand_unicode_or_cross_ascii_boundary`

Added inline tests in `hw/mod.rs`:

- `parallel_collection_reports_completion_and_shares_one_context`
- `provider_errors_and_panics_preserve_partial_output_and_diagnostics`
- `timeout_returns_without_joining_and_late_worker_can_finish`
- `title_filter_preserves_original_callback_index_and_skips_other_providers`
- `fallback_chain_records_order_and_short_circuits_at_first_success`
- `fallback_chain_returns_last_error_and_handles_an_empty_chain`
- `context_accessors_reuse_snapshots_and_retain_errors`
- `raw_report_literal_preserves_empty_and_untrimmed_bodies`

The remaining three passing tests are the existing wide-string tests. The collection tests use fabricated providers and a private deadline hook; they never enumerate hardware. The timeout test holds a worker behind a condition variable, proves the collection returns while it is still running, releases it, and confirms it can finish using its retained context. The parallel test requires both providers to start, releases the slower one from the faster one's completion callback, verifies callback order differs from result order, and checks one shared snapshot initialization.

## C# fixture confirmation and source evidence

The existing built helper was executed through `dotnet` in `--format` mode only:

```text
dotnet D:/GIT/HWID-Privacy-wt/golden-dump/app/tools/GoldenDump/bin/Release/net10.0-windows/win-x64/GoldenDump.dll --format D:/GIT/HWID-Privacy-wt/p1b-5/app/rust/tests/fixtures/phase1-report/formatter-cases.json
```

Exit 0. Its stdout is byte-identical to the concatenated `expected` fields: **17 calls, 2,503 UTF-8 bytes**. This covers headers, UTF-16 emoji/accented centering, section endings and empty content, labeled/combined lines, separators, and empty/nonempty device groups. No files in the GoldenDump worktree were changed or rebuilt. All identifiers in the fixture and tests are fabricated. The requested `TODO(1.9)` markers remain on formatting literal tests.

- `TextFormattingService.cs:13-81`: exact formatter behavior, including UTF-16 `PadLeft`, empty combined lines, and separators between empty groups.
- `SectionedViewForm.cs:544-548`: trim/empty-section placeholder behavior.
- `SectionedViewForm.cs:717-721`: the main-window export uses short headings and blank lines.
- `SectionedViewForm.cs:833` and `HardwareInfoManager.cs:51-52,99-103`: Old View uses the manager's complete raw report, including its main header and untrimmed section bodies.
- `HardwareInfoManager.cs:77`: standard per-provider failure text.
- `FileExportService.cs:25`: writes the supplied content directly; it does not reformat it.

The ordinal matching edge cases were also checked directly with the installed .NET runtime: `ß`/`SS`, `ß`/`ẞ`, dotless i/ASCII I, long s/ASCII S, and Kelvin sign/ASCII k compare unequal; the Greek iota-subscript lower/upper pair compares equal.

## Plan corrections and blockers

No relevant plan text required correction. The skeleton's `export_text` used the raw report's `FormatSection` layout, which differs from the actual main-window export at `SectionedViewForm.cs:717-721`; this implementation corrects that skeleton body while preserving its frozen signature. The handwritten raw-report test header was corrected to the C#-confirmed 33 leading spaces.

The worktree has no root `AGENTS.md` or `CLAUDE.local.md`; the main checkout's files were read instead, as already documented in the skeleton handoff. The plan files were read by absolute path and were not edited.

Blocked: none. Provider implementations, elevated live-hardware golden diffs, and UI integration remain their assigned work packages; this report claims formatting and collection foundation checks only.
