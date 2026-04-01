# UI Modernization Plan (HWID Checker)

## Execution Status (2026-02-09)
- Branch: `feature/ui-modernization`
- Phase A: completed
- Phase B: completed
- Phase C: in progress (kickoff + hierarchy pass 2)
- Phase D-E: pending
- Baseline worksheet: `ui-modernization-baseline-checklist.md`

### Completed So Far (A+B + C Kickoff)
1. Main window (`SectionedViewForm`) is now resizable with minimum size constraints and adaptive sidebar width.
2. Main action bar layout is responsive and wraps cleanly at narrower widths.
3. Text output areas now support horizontal scrolling where long hardware/log lines require it.
4. `CleanDevicesForm`, `CleanLogsForm`, and `WhitelistDevicesForm` were migrated to container-based adaptive layouts.
5. `DeviceRemovalConfirmationForm` was rebuilt on adaptive layout containers (no hard-coded absolute button positioning).
6. High-DPI spacing issues in modal/footer regions were fixed based on 4K@175% feedback.
7. Clean Logs UX improvements shipped:
   - immediate startup status lines
   - detailed progress reporting
   - deterministic end-of-run summary block
   - no auto-close after completion
8. Event-log cleaning pipeline hardening:
   - standard channels processed first
   - additional channel discovery with bounded parallel probing
   - explicit deduplication to prevent duplicate processing
9. Phase C kickoff implementation:
   - expanded UI color/button tokens in `ThemeColors`
   - shared button variants in `Buttons` (`Primary`, `Secondary`, `Danger`)
   - consistent action-button styling applied across main and modal forms
10. Phase C hierarchy pass 1:
   - main content now has a dedicated section header/title area
   - sidebar hierarchy improved with title/subtitle rhythm and clearer selected state
   - action bar button priority now uses mixed variants instead of one-color actions
11. Phase C hierarchy pass 2:
   - main action bar emphasis normalized (no unintended always-highlighted export/device-clean buttons)
   - clean windows now align more closely with main visual language
12. Usability hardening checkpoint:
   - `CleanLogsForm` now supports explicit force-stop and close during active runs
   - cancellation token now propagates from UI -> `SystemCleaningService` -> `EventLogCleaningService`
   - in-flight child processes are canceled/killed on user-requested stop

## Goal
Modernize the WinForms interface so it looks cleaner and more professional while staying functionally identical and working reliably across:
- Different resolutions
- Different Windows display scaling levels
- Different system font sizes
- Multi-monitor setups with mixed DPI

## Non-Negotiables
1. No regression in scan, export, cleaning, or update behavior.
2. Main UI remains fast and simple (no heavy framework migration).
3. Keep current update workflow untouched.
4. Preserve section-based navigation and current data semantics.

## Current Pain Points
1. Multiple forms are fixed-size/fixed-border and do not scale gracefully.
2. Many controls use hard-coded sizes (`Size = new Size(...)`) that can clip at high DPI.
3. Layout quality differs by resolution/scaling, especially button bars and modal dialogs.
4. Visual style is usable but inconsistent across forms.

## Phase A: Baseline and Test Matrix
Goal: Establish objective acceptance criteria before redesign.

Tasks:
1. Define UI test matrix (current scope):
   - 2560x1440 @ 100%
   - 3840x2160 @ 175%
   - 1920x1080 @ 100% (optional sanity check)
2. Capture baseline screenshots for:
   - Main window
   - Clean Devices
   - Clean Logs
   - Whitelist
   - Device removal confirmation
3. Record defects by form:
   - Clipped text
   - Overlapping controls
   - Wasted space
   - Poor keyboard flow

Exit criteria:
1. Baseline issues are documented and reproducible.
2. Required matrix rows are completed for 2K@100% and 4K@175%.

## Phase B: Layout System Hardening
Goal: Make forms resolution- and DPI-robust.

Tasks:
1. Standardize all forms on layout containers (`TableLayoutPanel`/`FlowLayoutPanel`) rather than absolute coordinates.
2. Replace fixed control widths/heights with:
   - `Dock`
   - `Anchor`
   - `AutoSize`
   - `MinimumSize`/`MaximumSize` where required
3. Main form:
   - Keep two-column structure
   - Allow adaptive sidebar width within bounded min/max
   - Make bottom action area wrap/stack elegantly at narrower widths
4. Dialogs/forms:
   - Convert fixed-size dialogs to adaptive layouts where safe
   - Keep minimum usable dimensions
5. Ensure all forms use consistent `AutoScaleMode` and runtime DPI behavior.

Exit criteria:
1. No clipping/overlap across test matrix.
2. All critical buttons remain visible and reachable.

## Phase C: Visual Redesign (Modern but Practical)
Goal: Improve clarity, hierarchy, and aesthetics without overdesign.

Status: in progress (consistency + hierarchy pass 2 implemented; next: typography/spacing polish).

Tasks:
1. Define UI tokens in one place:
   - Colors
   - Font sizes
   - Spacing scale
   - Corner radius style
2. Typography:
   - Improve hierarchy for headings, section labels, and body text
   - Keep monospaced fonts only where data readability benefits
3. Buttons:
   - Consistent size classes (primary/secondary/destructive)
   - Clear visual states (hover/pressed/disabled/focus)
4. Sidebar/content:
   - Stronger active section state
   - Better whitespace rhythm
   - Cleaner separators
5. Modal/dialog consistency:
   - Shared title/body/action spacing
   - Consistent warning/destructive affordances

Exit criteria:
1. Visual language is consistent across all forms.
2. Main workflows require fewer mis-clicks and feel cleaner.

## Phase D: Usability and Accessibility Pass
Goal: Make UI easier to operate under different user settings.

Tasks:
1. Keyboard:
   - Logical tab order
   - Visible focus indicators
   - Enter/Escape semantics on dialogs
2. Readability:
   - Validate contrast and font legibility
   - Ensure long lines wrap or scroll correctly
3. Responsiveness:
   - Prevent frozen-looking UI during long operations (status/progress clarity)
4. Error UX:
   - Standardize error/success messaging style and placement

Exit criteria:
1. All major workflows can be completed with keyboard + mouse.
2. No accessibility regressions introduced.

## Phase E: Validation and Rollout
Goal: Ship safely with confidence.

Tasks:
1. Re-run full resolution/scaling matrix.
2. Functional checks:
   - Scan
   - Export
   - Clean devices
   - Clean logs
   - Update check
3. Screenshot diff review between old and new UI.
4. Collect feedback from real users on mixed DPI setups.

Exit criteria:
1. UI passes matrix + functional checklist.
2. No critical layout defects remain.

## Suggested Order of Implementation
1. Phase A
2. Phase B
3. Phase C
4. Phase D
5. Phase E

## Deliverables
1. Updated UI forms with adaptive layouts.
2. Centralized visual tokens/style helpers.
3. Screenshot set for baseline vs modernized UI.
4. Short post-implementation QA checklist.
