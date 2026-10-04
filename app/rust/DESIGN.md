# HWIDChecker design system (Rust app)

This file is binding for every UI change in `app/rust/`. Read it before you touch `src/ui/`.

- `src/ui/theme.rs` holds every value (colors, fonts, sizes, radii). Form code uses only named constants from it.
- This file holds the rules and the reasons behind those values.
- A new design decision goes into this file in the same change that uses it. Never let code and this file disagree. If they do, fix one of them in the same change.

Origin: adapted on 2026-10-03 from a design handoff for a dark Rust launcher (zinc palette, shadcn style), given to the owner by a friend. The handoff's colors, type scale, button rules, and interaction patterns are the base. Parts that do not fit this app are listed in "Not adopted".

## 1. What is fixed and what is free

**Fixed by the C# app (do not change):** the set of windows, control order, button order and texts, message texts, keyboard and close rules, and every flow. The port must feel like the same program.

**Fixed at 100 percent DPI on a screen that fits the C# window:** window default and minimum sizes and resize behaviour, except the values this file names in sections 11 and 12.

**Free to change:** colors, fonts, borders, corner radii, hover, pressed and focus visuals, the window frame color, paddings and margins on the 4 px grid of section 12, the sidebar item height per section 11, and the icon glyphs of section 14 (the C# emoji prefixes of button texts). Every layout change needs an `approved-diffs.md` row before it ships.

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
| toggle states | none (`SECONDARY` off, `TEXT` on) |

Where the map applies today: the main window section meta line (section 13), the confirm dialog warning line (`WARNING`), the update window status line (`SECONDARY` for the byte counts, `SUCCESS` for `Download completed successfully`; section 16), and the message box icon (section 15). Text inside a multi-line text box is never colored. The loading state uses no status color: its title is `TEXT` and its counter `FAINT`, because loading is the expected state, not a notice.

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
- Where the C# fonts land: body 13/400 for the sidebar items, the whitelist header, the confirm message, and the message box text; 12/400 for the sidebar subtitle, the confirm warning (the C# italic is dropped), the loading counter, and the update status line; 11/600 for `Section {i} of {n}`; 15/600 for `Hardware Sections`, the loading title, and the update step text; 18/600 for the section title in the content header; 13/500 for every button, including the confirm dialog buttons (C# used 8.5 pt there).
- Inter 13 px is one pixel taller than Segoe UI 9 pt (`tmHeight` 16 against 15 at 96 DPI). Auto-size buttons therefore grow by one pixel and the footer row with them; the layout structure is unchanged.
- Sidebar items: 13 px / 400 at every tier, one line with an end ellipsis (section 11). The 14 captions do not elide on any listed setup; the ellipsis protects longer future titles and the minimum window.
- Data text wells (`EDIT`) use Consolas 10 pt (main window) and 9.75 pt (cleaners, whitelist), 9 pt in the Old View, no word wrap except the Old View, a 10 px inner margin left and right (`EM_SETMARGINS`, re-sent after every font change), and line height from the font. A well shows a scroll bar only when its text needs it (lines x line height against the client height, widest line against the client width; re-checked after every text change, resize and font change). A well never selects its text on focus; Ctrl+A still selects all. Text selection uses the system highlight color; a plain EDIT cannot restyle it.
- Icons in buttons are icon-font glyphs (section 14), never emoji.

- **Two wells keep their selection visible when unfocused** (`EditSpec::keep_selection()`, `ES_NOHIDESEL`): the main section well and the Old View well, so a find match stays visible while the find edit has the focus. Every other well keeps the C# `HideSelection = true` default.

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
- Multi-line text boxes and the whitelist list keep square corners with the 1 px `BORDER` frame painted over the native non-client border: the native scroll bars sit square in the corners, and the client area must not change. A `Fixed3D` box keeps its 2 px frame; the inner pixel takes the box's back color. Every text well uses this kit-painted frame (the `EditSpec` default); never a `WS_BORDER` (`FixedSingle`) EDIT, whose light frame lands inside the client area with the scroll bars outside it, and which has no non-client frame for the kit to repaint. The C# `FixedSingle` of the main section box (`SectionedViewForm.cs:214`) keeps the same 1 px outline; only the client inset grows from 1 px to the native edge width (`SM_CXEDGE`: 2 px at 96 DPI, 3 px at 144).
- No WinForms default-button ring: Enter still clicks the focused button or the accept button, the focus ring shows where the keyboard is.
- The progress bar is owner-painted as a pill: `HOVER` track and `TEXT` fill, both rounded by half the bar height (8 px tall in the update window), no edge; the fill is clipped by the track shape. A marquee shows a block one third wide that moves 4 px per 30 ms step with the clock, repainted by the control's own marquee timer.
- A container can be a card: 1 px `BORDER` outline, radius 8, over the parent's color (the section header). Three depths only: window (`BG`), card (`CARD`), text well.
- **Input (single-line edit).** 28 px tall, `CARD` fill, 1 px `BORDER` frame with radius 6 painted by the container (like the focus ring, offset 0), focus ring outside it, 13/400 `TEXT`, 10 px inner margins, cue text in the system gray (close to `FAINT`; accepted). The only editable text control of the app. `ES_AUTOHSCROLL`, no native frame or scroll bars; cue stays visible while focused, query limit 256; text changes deliver `Event::TextChanged(id)`.
- The focus ring follows the keyboard focus only: it disappears when the window is deactivated and comes back with the focus, like WinForms focus cues.
- The checked list keeps the native check glyph of the `DarkMode_Explorer` theme.
- `BORDER_STRONG` text selection applies to the list box (owner-drawn). The native multi-line EDIT paints its selection in the system highlight color, which a control cannot override. Accepted exception until the text boxes are owner-drawn; no well selects its text on its own, so the system color shows only after the user selects.

## 6. Buttons

- **Toggle.** A button can carry an on/off state. On: the CheckMark glyph (`E73E`) replaces an outline button's icon and adds the held look (`HOVER` fill, `BORDER_STRONG` outline). Sidebar host retains its leading icon, adds `TEXT` text and shows the CheckMark right-aligned, 12 px inset. Off: the host's rest look with its own icon. The accessible name is `{caption}, on` or `{caption}, off`; the drawn caption never changes. Space, Enter and click toggle. No status color: a toggle's state is not a status.
- **Icon-only button.** 28 x 28, icon centered, the text is the accessible name only (8.3).
- **Button text color override.** A button's text and icon can take a status color (`Form::set_button_fore`; `None` restores the kind's color). Fills and the disabled `FAINT` are unchanged. C8 uses `INFO` for the `Update available` notice.

- **Primary:** `TEXT` fill, `BG` text, no border. Hover `#E4E4E7`, pressed `#D4D4D8`. Disabled: `BORDER` fill, `FAINT` text. One per window: the main action.
- **Outline (default):** `CARD` fill, 1 px `BORDER`. Hover: `HOVER` fill and `BORDER_STRONG`. Pressed: `BORDER` fill. Disabled: `FAINT` text.
- **Destructive:** outline button with `DANGER` text. No red fill.
- **Sidebar item:** no fill at rest, `SECONDARY` text. Hover: `HOVER` fill, `SECONDARY` text. Active: `HOVER` fill, `TEXT` text, and a 2 px `TEXT` bar on the left edge inside the item (item height minus 2 x 4 px, radius 1). While a load runs, an item whose section is not collected yet shows `FAINT` text (section 13). No status color: `TEXT` carries "you are here", not a meaning.
- **Copy section:** a small outline button `Copy` (72 x 28, icon `Copy`) at the right edge of the section header, vertically centered. It copies the shown section body as CRLF text. Always enabled.
- **Focus:** keyboard focus shows the ring from section 5. A click also focuses the control, so focus is never lost.
- A disabled button needs a visible reason next to it or in its own text (for example `Loading...` with the History glyph on the Old View button).
- A disabled outline button keeps its `CARD` fill and `BORDER` outline; only the text turns `FAINT`.
- Sidebar items use the body font (13/400); every other button the button font (13/500). A sidebar item has no border and no WinForms image inset: its 12 px left padding is the whole inset, so the caption gets the full width.
- The one primary button per window: main window `Refresh` (the action that produces the data; C# styled all six as secondary); Device Cleaning `Reclean` (C# styled all three as primary); Log Cleaning `Close` / `Stop & Close` (the only button); whitelist `Save Whitelist` (the accept button); confirm dialog `Yes (Autoclose)` (the C# primary and accept button). The Old View and update windows have no buttons.
- The destructive button: `Reset Whitelist` (it deletes the whitelist file). The confirm dialog's `Yes` buttons are not destructive-styled: the dialog itself is the confirmation, and one of them is the primary.

## 7. Window frame

- **C5 Version.** Main window title `HWID Checker {CARGO_PKG_VERSION}`. `Cargo.toml` is the single source; `check.ps1` fails when any of `app.rc`'s four version fields differs. Other window and message-box titles are unchanged.

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
7. **Long text never breaks the layout.** Labels and sidebar items elide with `...`. Paragraphs word-wrap. Data text areas scroll.
8. **No flicker.** Buffered paint, one layout pass per resize and per DPI change (the main window's resize handler edits the tree only; the kit lays out once after it), no background erase under owner-drawn controls.
9. **Motion:** one loading indicator only (section 13), driven by a form timer. State changes are instant. The indicator stops when Windows "Show animations" is off (`SPI_GETCLIENTAREAANIMATION`, read when the control is created; the arc then stays at its start) and while the window is minimized (the kit kills every form timer on minimize). The progress marquee of the update window keeps its 30 ms clock (it is the native control's own timer and shows for less than a second).
10. **Nothing is larger than the screen.** Section 11.
11. **Find key routing.** Forms opt into `FormSpec::find_keys`: `Ctrl+F`, `F3`/`Shift+F3`, `Enter`/`Shift+Enter` in a single-line input, and `Esc` with the focused control id reach `Event::Key(FindKey)` before dialog navigation. The handler returns true only when consumed. Main and Old View currently use named no-op handlers; step 2b supplies find behavior. Neither sets `accept`/`cancel`.

## 9. Not adopted (and why)

- **Frameless window with custom caption buttons:** the native title bar keeps Snap Layouts, native resizing, and screen reader support with no extra code. The DWM colors give the same look.
- **Slint toolkit:** the app uses raw Win32 for a small exe with no runtime.
- **36 px control grid, toasts, badges, popups, nav rail icons:** the C# layout is fixed (section 1), and the app has no use for these parts.
- **Smaller sidebar text in tier D (12 px):** proposed by the UX pass; not adopted, 13 px still has 7 px of air in a 28 px row and a second font per item is not worth it.
- **Weight 500 on the active sidebar item:** the accent bar and `TEXT` are the cue; a second font per item is not worth it.
- **CJK font:** the UI is English only.

## 10. Checklist for any UI change

- Values only from `theme.rs`; no new literal colors or sizes in form code.
- Check at 100, 125, 150, 175 and 200 percent DPI and on the 1280x672 work area (1080p at 150 percent), and after a move between monitors with different scaling. The sidebar shows all 14 entries with no scrollbar in every case; no window is larger than the work area (`ui::main_window::live::fit_matrix` prints the table).
- Check keyboard use: Tab order, Enter, Esc, Space, focus ring visible.
- Check long text, an empty state, the loading state, and an error state.
- Check that the active sidebar item is identifiable with the window out of focus, and that a disabled button shows its reason.
- Update this file if the change adds or changes a design rule.

## 11. Screen fit

Logical work areas this app must be right on: 1920x1032, 1536x816, 2560x1392, 2048x1104, 2194x1186, 1280x672 (1920x1080 at 150 percent with the taskbar), plus custom scales. A window frame is client + 16 x 39 at 96 DPI (`AdjustWindowRectExForDpi` gives the exact value per DPI); a maximized client is work-area height - 23.

1. **Nothing is larger than the screen.** For every form, the default outer size, the minimum outer size (`WM_GETMINMAXINFO`), and the size Windows suggests on a DPI change are clamped to the work area of the monitor that holds the window; a window that hangs over an edge is moved inside. The clamp runs at creation, in the DPI change (before the one move, so the layout pass stays one), and again on `WM_DISPLAYCHANGE` and `WM_SETTINGCHANGE` (work area). Maximized and minimized windows are left to Windows. The main window still starts maximized when the unclamped default does not fit (AD-38); its restored size is the clamped one.
2. **The logical client size survives monitor moves.** The kit remembers the restored client size in 96-DPI pixels and answers `WM_GETDPISCALEDSIZE` with the frame of the new DPI around the scaled client, so a 920x640 client is 920x640 on every monitor (not the linear scale of the old outer size, which lost 2 px per move).
3. **All 14 sidebar entries are visible without a scrollbar on every listed work area.** The sidebar picks the first tier whose content fits its inner height (client height minus the footer and the 3 px cell margins): A (item 42, gap 4, subtitle shown; 722 px content at 96 DPI), B (36, 4, subtitle shown; 638), C (32, 3, subtitle hidden, title gap 6; 546), D (28, 2, subtitle hidden, title 28, gap 4; 468). Below tier D a scrollbar returns. Tier choice runs in the resize handler, one layout pass; the footer height comes from the last layout. Item text stays 13 px at every tier.
4. **Sidebar items are one line.** Width = sidebar width - cell margins - panel padding - scrollbar (once, only when it shows) - 12, all DPI-scaled, minimum 160 scaled. Longer captions elide. The inset is 12 (the UX pass proposed 16) because `NETWORK ADAPTERS (NIC's)` with its icon needs 228 px at 96 DPI and the minimum window gives the item 229.
5. **Footer** buttons keep their order and wrap only when the client is narrower than the row of six (about 800 px at 96 DPI); no listed work area does that. Spacing above and below the row is equal (8 px); wrapped rows get an 8 px gap.
6. **Old View** minimum outer size 640x400 (C# has none).
7. Strokes are device pixels: borders, the focus ring, the card outline, and the sidebar accent bar's radius never scale. The old 1 px divider row of the main window (which scaled to 2 px) is gone; the header card's outline replaces it.

8. **Sidebar tools block (C2, C8 scaffold).** Under the section list, outside its scroll flow: a 1 device px `BORDER` divider (margins 12 left/right, 8 above/below), then rows in the sidebar item look (13/400, icon at inset 12, 28 px tall, 2 px gap, same width as section items): the toggle `Startup Update Check` (icon Sync) and the action `Compare Exports` (icon Switch `E8AB`). No accent bar or loading state on tools rows. The block (75 px at 96 DPI; its stroke never scales) is subtracted before the tier is chosen; resulting tiers per listed work area are in `features-design.md` 0.2 (1536x816 goes A to B, 1280x672 C to D, the minimum window B to C). All 14 entries stay visible without a scrollbar everywhere; the two captions never elide. Tab order: sections, tools, footer. No extra bottom gap: the footer supplies it.

## 12. Spacing

Paddings and margins are multiples of 4: content 12, header card 12/8 with an 8 px gap to the well, sidebar item margin 12 left and right, 4 below, sidebar subtitle 8 below, footer 12/8 with 8 between buttons and none below them, dialog output and action panels 12 (the action row is 60 = 12 + 36 + 12), Old View well padding 12, update window 16, message box 20, sidebar item inset 12.

## 13. States of the main window

- **C8 Startup update check.** When the saved opt-in is on, check in parallel with the hardware load, with cancellation on owner close. No window, box or `Checking...` state during this check; errors are recorded only. An available result changes the idle `Updates` footer button to `Update available`, text and unchanged Sync icon in `INFO`, outline and hover fills unchanged. Keep one footer row at the minimum window (about 861 of 884 px). Manual intent takes priority over a late startup result, even if the manual flow has already finished. Closing releases the retained bytes.
- **Loading:** the content pane shows only the loading state, centered: the indicator (a 32 px ring in `BORDER` with a `TEXT` quarter arc, 3 px stroke, one turn per 1.2 s, repainted every 33 ms by a form timer), the title `Loading hardware information...` (15/600 `TEXT`, 16 px below the ring) and the counter `Collected {n} of 14 sections` (12/400 `FAINT`, 4 px below the title). The counter follows `hw::collect_all`'s `on_done` callback; the sections still fill all at the end (OPT-4). No text well, no scroll bars, no `Loading...` text is visible. Every sidebar item is `FAINT` until its section is collected, then `SECONDARY` again; the active item keeps `TEXT`. The footer stays as in C#.
- **Loaded:** the header card (title 18/600 `TEXT`, meta `Section {i} of {n}` 11/600, the `Copy` button at the right) and the text well.
- **Section meta color** by the body (status map): `INFO` while the body is `Loading...` (a load that failed leaves it so, like C#); `DANGER` when any line starts with `Error retrieving`; `WARNING` when a line contains `Unavailable (` or starts with `Error:` or `Error in`, and for the empty body `No data available`; otherwise `FAINT`. The title stays `TEXT`; the body is never colored.
- **Error:** a failed load shows its error in a message box (section 15) and leaves the loaded view with the `Loading...` bodies, as C# does.

## 14. Icons

Icons are glyphs of the Windows icon font, tinted like the text they sit next to: `Segoe Fluent Icons` on Windows 11, `Segoe MDL2 Assets` on Windows 10 (the same code points; no font file is shipped). The face is resolved once by creating the font and reading back the face GDI selected, because GDI substitutes an unknown face silently. 16 px at 100 percent in buttons and the sidebar, 8 px gap to the text; 24 px in message boxes. A button with an icon measures icon + gap + text, placed as one block (centered in footer and dialog buttons, left in the sidebar).

| Where | Glyph | Code |
|---|---|---|
| DISK DRIVES | HardDrive | `EDA2` |
| MOTHERBOARD | Component | `E950` |
| CHASSIS | PC1 | `E977` |
| (SM)BIOS | CommandPrompt | `E756` |
| SYSTEM INFORMATION | Info | `E946` |
| RAM MODULES | RAM | `EEA0` |
| CPU | CPU | `EEA1` |
| TPM MODULES | Lock | `E72E` |
| USB DEVICES | USB | `E88E` |
| GPU INFO | Game | `E7FC` |
| MONITOR INFORMATION | TVMonitor | `E7F4` |
| NETWORK ADAPTERS (NIC's) | Ethernet | `E839` |
| BLUETOOTH ADAPTERS | Bluetooth | `E702` |
| ARP INFO/CACHE | Network | `E968` |
| any other section | List | `EA37` |
| Refresh | Refresh | `E72C` |
| Export | Save | `E74E` |
| Clean Devices | Broom | `EA99` |
| Clean Logs | Delete | `E74D` |
| Updates / Checking... | Sync | `E895` |
| Old View / Loading... | History | `E81C` |
| Copy | Copy | `E8C8` |
| message box ring | StatusCircleRing | `F138` |
| information | StatusCircleInfo | `F13F` |
| warning | StatusCircleExclamation | `F13C` |
| error | StatusCircleErrorX | `F13D` |
| question | StatusCircleQuestionMark | `F142` |
| toggle on-state | CheckMark | `E73E` |
| Mask IDs (off; kit glyph reserved for step 2a) | Hide | `ED1A` |
| Compare Exports | Switch | `E8AB` |
| Startup Update Check | Sync | `E895` |
| find: previous (kit glyph for step 2b) | ChevronUp | `E70E` |
| find: next (kit glyph for step 2b) | ChevronDown | `E70D` |
| find: close (kit glyph for step 2b) | Cancel | `E711` |

The section lookup keeps the C# `GetSectionIcon` order (lower-case `Contains`, first match wins) with `chassis` and `bluetooth` matched before the generic words, so every section gets its own glyph. The C# button texts drop their emoji prefix (`↻ Refresh` is `Refresh` with the Refresh glyph; `⟳ Checking...` is `Checking...`); the texts after the prefix are unchanged.

## 15. Message boxes

Every `MessageBox.Show` of the C# app is a kit form of class `HWIDChecker.MessageBox` (the native box is the fallback only when the form cannot be created). Same title, text, buttons, and icon meaning:

- `BG` window, padding 20, the icon at the left top (24 px: the ring glyph with the status symbol stacked on it, in the status color: information `INFO`, warning `WARNING`, error `DANGER`, question `SECONDARY`), the text 13/400 `TEXT` word-wrapped at 440 px, a 20 px gap, the buttons right-aligned (88 x 34, margin 4 on every side so the focus ring has room, 8 px apart): OK; or Yes (primary) and No (outline). The default button is primary.
- The box is sized to its text at the DPI of the monitor it opens on (minimum 320 wide), modal to its owner, centered on it (or on the mouse's monitor without an owner), `CenterParent` rules of section 11 apply. The measuring DPI is the owner's; without an owner it is the DPI of the monitor under the cursor, read the way `Form::create` reads it (a hidden throwaway window at the cursor, because `GetDpiForMonitor` lives in shcore, which the exe does not import), so the wrap measured is the wrap shown on a secondary monitor with its own scaling.
- **Long text.** The wrapped text body is at most 360 px tall (`MSGBOX_BODY_MAX_HEIGHT`), so the box fits every work area of section 11 (client 106 + body at 96 DPI; outer 505 at most). When the wrapped text would be taller, the body becomes a read-only text well instead of a label: `CARD` fill, `BORDER` outline (1 device px), the 10 px inner margins of every well (section 4), the same 13/400 `TEXT`, word-wrapped at the full 440 px so only a vertical scroll bar appears (the kit shows it only when the text overflows, which is always the case here), 360 px tall, opened at the top. Width and height slack stay the same as the label (`MSGBOX_WIDTH_SLACK` 6, `MSGBOX_HEIGHT_SLACK` 4). A box whose text fits is byte-for-byte the label box of today; nothing else changes. Ctrl+C still copies the whole text through the hidden copy control; the well can be scrolled by mouse wheel, scroll bar, or keyboard once Tab reaches it.
- Keyboard like the native box: Enter presses the default button; Esc presses OK on an OK box; a Yes/No box has no Esc and its X is grayed (an answer is required); Ctrl+C copies the text; Tab moves between the buttons (and through the text well of a long box). Initial focus is the default button also when the box has a well.
- Owner rule unchanged (8.5): boxes raised after async work belong to the active window.
- Tests find a box by its class and title, read its text from the control with id `msgbox::TEXT_ID`, and press a button by posting `WM_COMMAND(IDOK | IDYES | IDNO, 0)` to the box; every click goes by HWND, never through the keyboard focus.

## 16. Update window

The startup notice click consumes the retained download and starts at the existing `Update Available` Yes/No prompt, without `Checking...` or another download. No releases it and restores `Updates` in its normal color. Yes uses the same replay, 500 ms completed hold and install path as a manual check; failure shows the existing `Update Error` box and restores the button. The ordinary manual check, its texts and the channel URL remain unchanged.

Outer 400 x 150 (scaled by DPI, AD-39), fixed. Padding 16. The app icon (32 px; the stock application icon when the exe has no icon resource, as in the test harness) at the left, 12 px gap, then one column that holds the step text (`Preparing download...`, `Downloading new version...`, `Preparing to restart...`; 15/600 `TEXT`, the window's heading), the status line under it (12/400; `SECONDARY` for the byte counts, `SUCCESS` for `Download completed successfully`), and the progress pill (8 px) 12 px below, so texts and bar share one left edge. Same texts, states and timing as AD-34; status color only on the status line. The progress bar is a painted control of the kit (no native progress class, so no classic 3D edge); its marquee runs on the control's own 30 ms timer.

## 17. Dialogs

- Device Cleaning, Log Cleaning: the text well inside a 12 px panel, the action row 60 px (12 px padding, buttons 36 px), buttons right-aligned 8 px apart. Busy state: the buttons are disabled with their C# texts; the well's own lines are the progress.
- Whitelist: header label 12 px above the list, the list in a 12 px panel, the same action row.
- Confirm Device Removal: message 13/400 `TEXT`, warning 12/400 `WARNING`, the three buttons centered; `Yes (Autoclose)` is the primary and accept button.
- Old View: the well in a 12 px panel, minimum 640 x 400, opens unselected at the top.
- Native open-file picker: `win::dialog::open_file(owner, title, filters, initial_dir)` uses `IFileOpenDialog`; cancel does nothing, errors are recorded. Native chrome keeps the Windows theme (section 9).
- Portable settings: `HWIDChecker.settings.json` next to the exe; pretty JSON, CRLF, UTF-8 without BOM, atomic replacement, unknown keys retained. `check_updates_on_start` defaults false on missing/corrupt/unreadable data (record the error). The sidebar reads on creation and saves immediately; failure reverts the toggle and shows `Settings Error` / `Could not save settings: {error}` / error icon / OK. Turning it on affects the next start only; `Updates` checks now. Compare Exports is a no-op scaffold until step 2d.

## 18. Features in design (2026-10-04; not in code yet)

The rules below are binding for the features C1, C2, C3, C4, C5 and C8 of `docs/rust-port/IMPROVEMENTS.md`. The detail (sizes, texts, flows, states) is in `docs/rust-port/features-design.md`. When a feature lands, move its rules into the sections above and delete them here, so code and this file never disagree (line 7). Until then, nothing in this section describes the shipped build.

### 18.3 Per feature

- **C1 Mask IDs.** Outline toggle `Mask IDs` (icon Hide `ED1A`, min 96 x 28) in the section header card, 8 px left of `Copy`. On: identifiers recorded by the providers (`Section.ids`, 4 characters or longer) are replaced in the view, Copy, Export, and Old View by `X` per ASCII alphanumeric character; separators and length stay (`XX:XX:XX:XX:XX:XX`, `{XXXXXXXX-XXXX-...}`). Not persisted; off at every start. Masked exports are named `-MASKED`.
- **C2 Compare Exports.** Two open-file dialogs (before, after), then a modal window `Compare Exports` (1000 x 700 scaled, minimum 640 x 400): the header card with `Before`/`After` file names (11/600 `SECONDARY` labels, 13/400 `TEXT` names, ellipsis), the summary `Changed n · Added n · Removed n · Same n` in 11/600 `FAINT`, `Copy` 72 x 28; the well (`CARD`, Consolas 10 pt, no wrap) lists rows per section with a kind column `changed` / `removed` / `added` / `same`. No colors in rows (section 3) and none on the summary: a count is not a status and the tool cannot know whether a change is good. Esc closes.
- **C3 JSON export.** `Export` writes the `.txt` and a `.json` with the same stamp in one click; the success text gains `JSON: {path}`. No new control, no dialog, no footer change.
- **C4 Find.** A 28 px bar between the header card and the well (main) or above the well (Old View), 8 px gap below, hidden until `Ctrl+F`: input (fills, min 160), count `{i} of {n}` / `No matches` in 12/400 `SECONDARY`, icon-only `Previous match` (ChevronUp `E70E`), `Next match` (ChevronDown `E70D`) 4 px apart, `Close find` (Cancel `E711`); gaps 8. A match is the well's selection (the system highlight, section 5 exception). Previous/Next are disabled at 0 matches; the count label is the reason.

### 18.5 Status map additions (section 3)

| Meaning | Color |
|---|---|
| update available (footer button notice) | `INFO` |
| compare counts, find counts | none (`FAINT` / `SECONDARY` text) |
