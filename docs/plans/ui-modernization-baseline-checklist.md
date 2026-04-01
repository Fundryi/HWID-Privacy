# UI Modernization Baseline Checklist

## Purpose
Capture reproducible baseline behavior before UI redesign changes.

## Current Status (2026-02-09)
1. Baseline captured and used to drive layout fixes in Phase B.
2. 4K @ 175% (`M2`) has been actively validated with real screenshots across all target forms.
3. Additional verification on 2K @ 100% (`M1`) should be re-run once after the latest modal/log-cleaning and hierarchy updates to close the loop.

## Test Matrix

| ID | Resolution | Scaling | Font Size | Pass/Fail | Notes |
|---|---|---|---|---|---|
| M1 | 2560x1440 | 100% | Default | In Progress | Primary real-device test; quick recheck pending after latest changes |
| M2 | 3840x2160 | 175% | Default | Pass | Verified through iterative screenshot feedback and fixes |
| M3 | 1920x1080 | 100% | Default | Optional | Optional sanity check (common user setup) |

## Forms to Validate
1. Main window (`SectionedViewForm`)
2. Clean Devices (`CleanDevicesForm`)
3. Clean Logs (`CleanLogsForm`)
4. Whitelist (`WhitelistDevicesForm`)
5. Device removal confirmation (`DeviceRemovalConfirmationForm`)

## Per-Form Checks
1. No clipped text
2. No overlapping controls
3. Buttons visible and reachable
4. Scrollbars behave correctly
5. Keyboard navigation (Tab/Enter/Escape) is logical
6. Visual hierarchy is readable at selected scaling

## Screenshot Capture Log

| Matrix ID | Form | Screenshot Path | Notes |
|---|---|---|---|
| M1 | Main window | Pending refresh | Recheck requested after latest hierarchy/button updates |
| M1 | Clean Devices | Pending refresh | Recheck requested after latest styling updates |
| M1 | Clean Logs | Pending refresh | Recheck requested after latest cancellation/force-stop updates |
| M1 | Whitelist | Pending refresh | Recheck requested after latest updates |
| M1 | Device removal confirmation | Pending refresh | Recheck requested after latest updates |
| M2 | Main window | Captured (user run) | No clipping; horizontal scroll behavior confirmed |
| M2 | Clean Devices | Captured (user run) | Footer sizing fixed; output visibility confirmed |
| M2 | Clean Logs | Captured (user run) | Footer sizing + persistent summary flow confirmed |
| M2 | Whitelist | Captured (user run) | Header/action spacing corrected |
| M2 | Device removal confirmation | Captured (user run) | Adaptive button/message layout confirmed |

## Defect Log

| Defect ID | Matrix ID | Form | Severity | Issue | Repro Steps | Suggested Fix |
|---|---|---|---|---|---|---|
| D-001 | M2 | Clean Logs | High | Blank/unclear feedback during startup stage | Run clean logs on 4K@175% and observe no early output | Added immediate startup status and detailed progress events |
| D-002 | M2 | Clean Logs/Clean Devices | Medium | Oversized footer area after layout migration | Open forms on 4K@175% and inspect bottom action row | Fixed action row height/margins and disabled wrapping for single-line button rows |
| D-003 | M2 | Whitelist | Medium | Excessive top whitespace in header area | Open whitelist form on 4K@175% | Flattened header container and tightened spacing |
| D-004 | M2 | Clean Logs | High | User could not reliably stop/close while active cleaning run was in progress | Start log cleaning and attempt to close via button/titlebar | Added force-stop confirmation + cancellation token propagation through cleaning pipeline |
| D-005 | M2 | Main window | Low | `Export` and `Clean Devices` appeared always highlighted, causing ambiguous primary action emphasis | Open main form after hierarchy pass 1 | Normalized action button variants to secondary style for balanced emphasis |

## Phase A Exit Criteria
1. All matrix rows executed and logged.
2. Baseline screenshots captured for each form in M1 and M2.
3. Defects are reproducible and mapped to target phases (B/C/D).

Current assessment:
1. Criteria 1 and 2 are complete for M2.
2. Criteria 3 is complete and defects have been fixed/mapped.
3. One M1 sanity re-run remains to fully close this checklist.
