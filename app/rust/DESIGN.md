# HWIDChecker design system (Rust app)

This file is binding for every UI change in `app/rust/`. Read it before you touch `src/ui/`.

- `src/ui/theme.rs` holds every value (colors, fonts, sizes, radii). Form code uses only named constants from it.
- This file holds the rules and the reasons behind those values.
- A new design decision goes into this file in the same change that uses it. Never let code and this file disagree. If they do, fix one of them in the same change.

Origin: adapted on 2026-10-03 from a design handoff for a dark Rust launcher (zinc palette, shadcn style), given to the owner by a friend. The handoff's colors, type scale, button rules, and interaction patterns are the base. Parts that do not fit this app are listed in "Not adopted".

## 1. What is fixed and what is free

**Fixed by the C# app (do not change):** window layout, control order, button positions, button texts, window default and minimum sizes, resize behavior, keyboard and close rules, message texts, and every flow. The port must feel like the same program.

**Free to change:** colors, fonts, borders, corner radii, hover, pressed and focus visuals, and the window frame color. Text width can change slightly with the font. The layout structure must stay the same.

## 2. Direction

- Dark only. Neutral zinc surfaces.
- Color carries meaning only (status). It is never decoration.
- One primary (white) button per window. Every other button is an outline button.
- Dense and calm: thin 1 px borders, no gradients, no shadows inside windows.
- Hardware data, logs, and the raw report stay in a monospace font, because the report uses column alignment (disk tree, RAM table).

## 3. Color tokens

| Token | Value | Use |
|---|---|---|
| `BG` | `#09090B` | window background, sidebar, title bar |
| `CARD` | `#111114` | panels, text boxes, list boxes, outline button at rest |
| `HOVER` | `#18181B` | hover fill, selected sidebar item, progress track |
| `BORDER` | `#27272A` | every divider and outline |
| `BORDER_STRONG` | `#3F3F46` | hovered outline, dialog outline, text selection |
| `TEXT` | `#FAFAFA` | primary text |
| `SECONDARY` | `#A1A1AA` | supporting text, inactive icons, focus ring |
| `FAINT` | `#71717A` | captions, placeholders, disabled text |
| `SUCCESS` | `#4ADE80` | done, removed, cleared, up to date |
| `WARNING` | `#FBBF24` | skipped, cancelled, dry run, presence unclear |
| `DANGER` | `#F87171` | failed, error, destructive action text |
| `INFO` | `#60A5FA` | running, loading, notices |

Fixed one-off values: primary button hover `#E4E4E7`, pressed `#D4D4D8`. Log and raw-report text well `#0C0C0E`.

The C# names in `theme.rs` (for example `MAIN_BACKGROUND`, `SIDEBAR_ITEM_ACTIVE`) stay as names so the code still maps to `ThemeColors.cs`. Their values point to the tokens above.

Status map (use the color by meaning, never by the text):

| Meaning | Color |
|---|---|
| done, removed, cleared, up to date | `SUCCESS` |
| failed, error | `DANGER` |
| skipped, cancelled, dry run, unclear | `WARNING` |
| running, loading, notice | `INFO` |
| anything else | `FAINT` |

## 4. Type

- UI font: **Inter**, built into the exe (`assets/fonts/`, OFL license in `Inter-OFL.txt`). Weights Regular 400, Medium 500, SemiBold 600. Loaded at startup with `AddFontMemResourceEx`, so it never depends on an installed font or a download. If loading fails, the app records it and falls back to Segoe UI.
- Data font: **Consolas** for hardware sections, cleaner output, the whitelist list, and Old View.

| Size | Weight | Use |
|---|---|---|
| 11 px | 600 | small captions, section meta |
| 12 px | 400 | secondary lines, sidebar subtitles, hints |
| 13 px | 400 / 500 | body, buttons (500), inputs |
| 15 px | 600 | section titles |
| 18 px | 600 | window titles inside content |

Sizes are logical pixels at 96 DPI and scale with DPI.

## 5. Shape

| Item | Value |
|---|---|
| Button and input corner radius | 6 |
| Panel, dialog corner radius | 8 |
| Border width | 1 px, always |
| Focus ring | 1 px `SECONDARY`, 3 px outside the control, radius 8 |
| Rounded corners | anti-aliased (drawn into a 32-bit buffer, never plain GDI `RoundRect`) |

Control sizes and paddings come from the C# layout (section 1), not from the handoff's 36 px grid.

## 6. Buttons

- **Primary:** `TEXT` fill, `BG` text, no border. Hover `#E4E4E7`, pressed `#D4D4D8`. Disabled: `BORDER` fill, `FAINT` text. One per window: the main action.
- **Outline (default):** `CARD` fill, 1 px `BORDER`. Hover: `HOVER` fill and `BORDER_STRONG`. Pressed: `BORDER` fill. Disabled: `FAINT` text.
- **Destructive:** outline button with `DANGER` text. No red fill.
- **Sidebar item:** no fill at rest, `SECONDARY` text. Hover `HOVER` fill. Active: `HOVER` fill and `TEXT` text. No accent color.
- **Focus:** keyboard focus shows the ring from section 5. A click also focuses the control, so focus is never lost.
- A disabled button needs a visible reason next to it or in its own text (for example `📜 Loading...`).

## 7. Window frame

- Native title bar, not frameless. On Windows 11: `DwmSetWindowAttribute` sets the caption color to `BG`, the border color to `BORDER`, and the caption text color to `TEXT` (attributes 35, 34, 36), plus immersive dark mode. On Windows 10: immersive dark mode only.
- Windows 11 rounds the window corners itself.
- Every window shows the app icon, large and small (`hIcon` and `hIconSm`, the small one loaded at the small-icon size so it stays sharp).

## 8. Interaction patterns

1. **Data in, intent out.** A form is declared as data (`FormSpec`, `Node` tree). Logic runs on workers and posts results back with a window generation. Stale results are dropped.
2. **Focus is never lost.** When the focused control is disabled, focus moves to the next tab stop, like WinForms.
3. **Keyboard works everywhere.** Every clickable control is a real `BUTTON` (Space and Enter work, screen readers get a name). Icon-only buttons still carry a text name.
4. **Modal dialogs own the keyboard.** Enter and Esc follow the C# rules for that dialog.
5. **Message boxes belong to the active window.** If another modal window is open, the box uses it as owner, so closing the box never re-enables a disabled window.
6. **Hidden means idle.** No timer runs and nothing repaints while a window is minimized.
7. **Long text never breaks the layout.** Labels elide with `...`. Paragraphs word-wrap. Data text areas scroll.
8. **No flicker.** Buffered paint, one layout pass per resize, no background erase under owner-drawn controls.
9. **Motion:** none. State changes are instant. If an animation is ever added, it must turn off when Windows "Show animations" is off (`SPI_GETCLIENTAREAANIMATION`).

## 9. Not adopted (and why)

- **Frameless window with custom caption buttons:** the native title bar keeps Snap Layouts, native resizing, and screen reader support with no extra code. The DWM colors give the same look.
- **Slint toolkit:** the app uses raw Win32 for a small exe with no runtime.
- **36 px control grid, toasts, badges, popups, nav rail icons:** the C# layout is fixed (section 1), and the app has no use for these parts.
- **CJK font:** the UI is English only.

## 10. Checklist for any UI change

- Values only from `theme.rs`; no new literal colors or sizes in form code.
- Check at 100, 150, and 200 percent DPI, and after a move between monitors.
- Check keyboard use: Tab order, Enter, Esc, Space, focus ring visible.
- Check long text and an empty state.
- Update this file if the change adds or changes a design rule.
