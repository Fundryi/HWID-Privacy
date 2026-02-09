# UI Modernization Baseline Checklist

## Purpose
Capture reproducible baseline behavior before UI redesign changes.

## Test Matrix

| ID | Resolution | Scaling | Font Size | Pass/Fail | Notes |
|---|---|---|---|---|---|
| M1 | 1366x768 | 100% | Default | TBD | |
| M2 | 1920x1080 | 100% | Default | TBD | |
| M3 | 1920x1080 | 125% | Default | TBD | |
| M4 | 2560x1440 | 100% | Default | TBD | |
| M5 | 2560x1440 | 150% | Default | TBD | |
| M6 | 3840x2160 | 150% | Default | TBD | |
| M7 | 3840x2160 | 200% | Default | TBD | |
| M8 | 1920x1080 | 125% | Larger text | TBD | |

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
| M1 | Main window | TBD | |
| M1 | Clean Devices | TBD | |
| M1 | Clean Logs | TBD | |
| M1 | Whitelist | TBD | |
| M1 | Device removal confirmation | TBD | |

## Defect Log

| Defect ID | Matrix ID | Form | Severity | Issue | Repro Steps | Suggested Fix |
|---|---|---|---|---|---|---|
| D-001 | TBD | TBD | TBD | TBD | TBD | TBD |

## Phase A Exit Criteria
1. All matrix rows executed and logged.
2. Baseline screenshots captured for each form in at least M1, M3, M5, and M7.
3. Defects are reproducible and mapped to target phases (B/C/D).
