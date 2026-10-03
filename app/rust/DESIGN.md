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

Which text areas use `CARD` and which use the text well: the hardware section text and the whitelist list are `CARD` (data the user reads and selects); the Device Cleaning and Log Cleaning output boxes and the Old View raw report are the text well (logs and the raw dump).

The C# names in `theme.rs` (for example `MAIN_BACKGROUND`, `SIDEBAR_ITEM_ACTIVE`) stay as names so the code still maps to `ThemeColors.cs`. Their values point to the tokens above. The update window, which kept the system colors in C#, uses `BG` and `TEXT` like every other window.

Status map (use the color by meaning, never by the text):

| Meaning | Color |
|---|---|
| done, removed, cleared, up to date | `SUCCESS` |
| failed, error | `DANGER` |
| skipped, cancelled, dry run, unclear | `WARNING` |
| running, loading, notice | `INFO` |
| anything else | `FAINT` |

Where the map applies today: the main window loading overlay (`INFO`), the confirm dialog warning line (`WARNING`), the update window status label (`INFO` while it runs) and its detail line (`TEXT` for the byte counts, `SUCCESS` for `Download completed successfully`). Text inside a multi-line text box is never colored.

## 4. Type

- UI font: **Inter**, built into the exe (`assets/fonts/`, OFL license in `Inter-OFL.md`; a `.txt` name would be caught by the repo `*.txt` ignore rule). Weights Regular 400, Medium 500, SemiBold 600. Registered once with `AddFontMemResourceEx` when the first font is created (inside `Form::create`, before the first window shows), so it never depends on an installed font or a download. If loading fails, the app records it and falls back to Segoe UI.
- Data font: **Consolas** for hardware sections, cleaner output, the whitelist list, and Old View.

| Size | Weight | Use |
|---|---|---|
| 11 px | 600 | small captions, section meta |
| 12 px | 400 | secondary lines, sidebar subtitles, hints |
| 13 px | 400 / 500 | body, buttons (500), inputs |
| 15 px | 600 | section titles |
| 18 px | 600 | window titles inside content |

Sizes are logical pixels at 96 DPI and scale with DPI. `theme.rs` stores them as points (`px * 0.75`: 11 px = 8.25 pt, 13 px = 9.75 pt, 15 px = 11.25 pt, 18 px = 13.5 pt), so GDI lands on the exact pixel size at every DPI.

- Inter's weights are separate one-style GDI families: `Inter` (400), `Inter Medium` (500), `Inter SemiBold` (600). `dpi::Font` maps the weight to the face; the code never asks GDI to synthesize a weight.
- Fallback mapping when the embedded fonts fail to register: 400 and 500 become `Segoe UI`, 600 becomes `Segoe UI Semibold`. The substitution is explicit because GDI would map an unknown face to a default font, not to Segoe UI.
- Where the C# fonts land: body 13/400 for the sidebar items, the whitelist header, the confirm message, the loading overlay, and the update status label; 12/400 for the sidebar subtitle, the confirm warning (the C# italic is dropped), and the update detail line; 11/600 for `Section {i} of {n}`; 15/600 for `Hardware Sections`; 18/600 for the section title in the content header; 13/500 for every button, including the confirm dialog buttons (C# used 8.5 pt there).
- Inter 13 px is one pixel taller than Segoe UI 9 pt (`tmHeight` 16 against 15 at 96 DPI). Auto-size buttons therefore grow by one pixel and the footer row with them; the layout structure is unchanged.
- Emoji in button texts render through GDI font fallback with Inter as they did with Segoe UI (verified, no run splitting needed).

## 5. Shape

| Item | Value |
|---|---|
| Button and input corner radius | 6 |
| Panel, dialog corner radius | 8 |
| Border width | 1 px, always |
| Focus ring | 1 px `SECONDARY`, 3 px outside the control, radius 8 |
| Rounded corners | anti-aliased (drawn into a 32-bit buffer, never plain GDI `RoundRect`) |

Control sizes and paddings come from the C# layout (section 1), not from the handoff's 36 px grid.

- Anti-aliased shapes are drawn with tiny-skia (pure Rust, CPU only, no DLL import) into a 32-bit DIB and copied to the DC; text is drawn by GDI on top.
- Radii and the focus ring offset are logical values and scale with DPI (6 px becomes 9 px at 150 percent). Strokes (borders, focus ring, dividers) stay one device pixel at every DPI.
- The focus ring sits outside the control, so the container window paints it around its focused button (the button's own window cannot paint outside itself). The C# margins (at least 5 px between controls) leave room for the 3 px offset plus the 1 px ring.
- Multi-line text boxes and the whitelist list keep square corners with the 1 px `BORDER` frame painted over the native non-client border: the native scroll bars sit square in the corners, and the client area must not change. A `Fixed3D` box keeps its 2 px frame; the inner pixel takes the box's back color.
- No WinForms default-button ring: Enter still clicks the focused button or the accept button, the focus ring shows where the keyboard is.
- The progress bar is owner-painted: `HOVER` track, `TEXT` fill, no edge. A marquee shows a block one third wide that moves 4 px per 30 ms step with the clock, repainted by the control's own marquee timer.
- The focus ring follows the keyboard focus only: it disappears when the window is deactivated and comes back with the focus, like WinForms focus cues.
- The checked list keeps the native check glyph of the `DarkMode_Explorer` theme.
- `BORDER_STRONG` text selection applies to the list box (owner-drawn). The native multi-line EDIT paints its selection in the system highlight color, which a control cannot override; the Old View's select-all on open therefore shows the system blue. Accepted exception until the text boxes are owner-drawn.

## 6. Buttons

- **Primary:** `TEXT` fill, `BG` text, no border. Hover `#E4E4E7`, pressed `#D4D4D8`. Disabled: `BORDER` fill, `FAINT` text. One per window: the main action.
- **Outline (default):** `CARD` fill, 1 px `BORDER`. Hover: `HOVER` fill and `BORDER_STRONG`. Pressed: `BORDER` fill. Disabled: `FAINT` text.
- **Destructive:** outline button with `DANGER` text. No red fill.
- **Sidebar item:** no fill at rest, `SECONDARY` text. Hover `HOVER` fill. Active: `HOVER` fill and `TEXT` text. No accent color.
- **Focus:** keyboard focus shows the ring from section 5. A click also focuses the control, so focus is never lost.
- A disabled button needs a visible reason next to it or in its own text (for example `📜 Loading...`).
- A disabled outline button keeps its `CARD` fill and `BORDER` outline; only the text turns `FAINT`.
- Sidebar items use the body font (13/400); every other button the button font (13/500).
- The one primary button per window: main window `↻ Refresh` (the action that produces the data; C# styled all six as secondary); Device Cleaning `Reclean` (C# styled all three as primary); Log Cleaning `Close` / `Stop & Close` (the only button); whitelist `Save Whitelist` (the accept button); confirm dialog `Yes (Autoclose)` (the C# primary and accept button). The Old View and update windows have no buttons.
- The destructive button: `Reset Whitelist` (it deletes the whitelist file). The confirm dialog's `Yes` buttons are not destructive-styled: the dialog itself is the confirmation, and one of them is the primary.

## 7. Window frame

- Native title bar, not frameless. On Windows 11: `DwmSetWindowAttribute` sets the caption color to `BG`, the border color to `BORDER`, and the caption text color to `TEXT` (attributes 35, 34, 36), plus immersive dark mode. On Windows 10: immersive dark mode only.
- Windows 11 rounds the window corners itself.
- Every window shows the app icon, large and small (`hIcon` and `hIconSm`, the small one loaded at the small-icon size so it stays sharp).
- A failed `DwmSetWindowAttribute` call is recorded (`win::record`) and the window keeps the system frame; it never stops the window from opening.

## 8. Interaction patterns

1. **Data in, intent out.** A form is declared as data (`FormSpec`, `Node` tree). Logic runs on workers and posts results back with a window generation. Stale results are dropped.
2. **Focus is never lost.** When the focused control is disabled, focus moves to the next tab stop, like WinForms.
3. **Keyboard works everywhere.** Every clickable control is a real `BUTTON` (Space and Enter work, screen readers get a name). Icon-only buttons still carry a text name.
4. **Modal dialogs own the keyboard.** Enter and Esc follow the C# rules for that dialog.
5. **Message boxes belong to the active window.** If another modal window is open, the box uses it as owner, so closing the box never re-enables a disabled window. `msgbox::active_window()` is the owner for every box raised after async work (load results, the update flow, cleaner results); a box raised directly from a click keeps the form as owner.
6. **Hidden means idle.** No timer runs and nothing repaints while a window is minimized. The kit kills every form timer on minimize and restarts it with the same interval on restore; a timer set while minimized starts on restore. Owned windows (the update progress window, the modal cleaners) are hidden with their owner but not minimized themselves, so an update install never stalls.
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
