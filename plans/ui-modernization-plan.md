# UI Modernization Plan (HWID Checker)

## Execution Status (2026-02-09)
- Branch: `feature/ui-modernization`
- Phase A: in progress
- Phase B-E: pending
- Baseline worksheet: `plans/ui-modernization-baseline-checklist.md`

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
1. Define UI test matrix:
   - 1366x768 @ 100%
   - 1920x1080 @ 100% / 125%
   - 2560x1440 @ 100% / 150%
   - 3840x2160 @ 150% / 200%
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
