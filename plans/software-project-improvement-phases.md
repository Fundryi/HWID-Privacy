# Software-Project Improvement Plan (Phased)

## Scope
`HWID-Checkers/Software-Project` audit and improvement roadmap to make the project more consistent, maintainable, and production-safe while preserving current behavior (scan HWIDs, display results, export data).

## Execution Status (2026-02-09)
- Phase 1: completed
- Phase 2: completed
- Phase 3: completed (service hardening + cleanup)
- Phase 4: completed (minimal hardening only; updater workflow kept simple)
- Phase 5: pending

## Current-State Findings
- Docs were partially out of sync with implementation (comparison references, old update mechanism details, stale paths).
- Legacy UI path (`MainForm*`) and legacy data handlers remain in-tree but are not used by the active entry point.
- Auto-update service works, but trust model is weak (hash compare only, no signed artifact verification).
- Event log cleaner process output handling is fragile (`RunProcessAsync` currently captures only a single output/error line).
- Several services/classes contain dead or confusing code paths (unused constants/fields, backward-compat wrapper types that are likely no longer needed).

## Phase 1: Documentation and Structure Hygiene
Goal: Make repo docs authoritative for current code.

Tasks:
1. Keep `README.md`, `AI-README.md`, and `AUTO-UPDATE-README.md` aligned with active architecture.
2. Add a lightweight "source of truth" checklist for future doc updates (trigger-based: when changing UI entry point, update service flow docs, etc.).
3. Keep removal plans (`plans/*.md`) status-tagged (planned/in-progress/completed) to avoid stale execution plans.

Validation:
1. No comparison-related features documented in active project docs.
2. Build and run instructions work exactly as written.

## Phase 2: Remove or Isolate Legacy Code Paths
Goal: Reduce maintenance surface and accidental regressions.

Tasks:
1. Delete unused legacy files:
   - `source/UI/Forms/MainForm.cs`
   - `source/UI/Forms/MainFormLayout.cs`
   - `source/UI/Forms/MainFormInitializer.cs`
   - `source/UI/Forms/MainFormEventHandlers.cs`
   - `source/UI/Forms/MainFormLoader.cs`
   - `source/UI/Forms/MainForm.resx`
   - `source/UI/DataHandlers/HardwareDataHandler.cs`
   - `source/UI/DataHandlers/HardwareDataHandlerFactory.cs`
   - `source/UI/DataHandlers/NetworkInfoHandler.cs`
   - `source/UI/DataHandlers/SystemInfoHandler.cs`
2. Remove stale references/comments tied to deleted legacy code.
3. Re-run publish and sanity-check main flows.

Validation:
1. `dotnet publish` succeeds.
2. Program entry still launches `SectionedViewForm` correctly.
3. No compile references remain to removed classes.

## Phase 3: Reliability and Correctness Hardening
Goal: Improve behavior under failure and edge cases.

Tasks:
1. Fix `EventLogCleaningService.RunProcessAsync` to accumulate full stdout/stderr, not a single line.
2. Refactor `DeviceCleaningService` ghost-device detection logic for explicit, verifiable criteria.
3. Replace placeholder `deviceName = "True"` with real device naming field(s) to improve operator visibility.
4. Remove unused parameters/constants/fields (example: auto-update constants not consumed, constructor-injected fields not used).
5. Add cancellation/timeouts for long-running shell operations where appropriate.

Validation:
1. Log cleaner reports consistent results across multiple logs.
2. Device cleaning output contains meaningful device names.
3. No dead code warnings for removed symbols.

## Phase 4: Update Pipeline Security Hardening
Goal: Make updater safer and more deterministic.

Tasks:
1. Move from SHA1 to SHA256 for binary integrity checks.
2. Keep existing workflow: commit `HWIDChecker.exe` and update via current update button path.
3. Defer release-channel/signature/manifest complexity unless explicitly requested.
4. Keep rollback/restart behavior unchanged unless reliability issues are observed.

Validation:
1. Update check still works with new hash scheme.
2. Corrupted or tampered download is rejected.
3. Failure path leaves existing executable runnable.

## Phase 5: Test and CI Baseline
Goal: Prevent regressions and reduce manual verification load.

Tasks:
1. Add unit tests for formatter and export naming behavior.
2. Add parser/section tests for `SectionedViewForm` section extraction logic.
3. Add integration-style tests for service-level logic where mockable (without requiring admin/hardware mutation).
4. Add CI workflow to run build + tests on push/PR.

Validation:
1. CI passes on clean checkout.
2. Critical service behaviors are covered by tests.

## Suggested Execution Order
1. Phase 1
2. Phase 2
3. Phase 3
4. Phase 4
5. Phase 5

## Notes
- Phase 2 (legacy deletion) is the biggest complexity reduction for minimal functional risk.
- Phase 3 should start with process-output handling and device-cleaning observability because those affect operational trust immediately.
