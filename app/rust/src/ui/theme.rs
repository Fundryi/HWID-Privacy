//! Owned by WP-10a (values by WP-18): every color, font, size, padding and margin of the forms.
//!
//! Colors and fonts are the `DESIGN.md` tokens. The C# names from `UI/Components/ThemeColors.cs`
//! and the form constants stay as names and point at the tokens, so the code still maps to the
//! C# forms. Sizes are logical pixels at 96 DPI; `dpi::scale` converts them. Form code uses these
//! names, never literals.

use super::layout::{Pad, Size};
use windows::Win32::Foundation::COLORREF;

/// An opaque RGB color, like `System.Drawing.Color.FromArgb(r, g, b)`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Color {
    /// Red channel.
    pub r: u8,
    /// Green channel.
    pub g: u8,
    /// Blue channel.
    pub b: u8,
}

impl Color {
    /// Builds a color from its three channels.
    pub const fn rgb(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b }
    }

    /// Builds a color from a `0xRRGGBB` value (the `DESIGN.md` notation).
    pub const fn hex(v: u32) -> Self {
        Self::rgb((v >> 16) as u8, (v >> 8) as u8, v as u8)
    }

    /// Returns the GDI `COLORREF` (0x00BBGGRR).
    pub const fn colorref(self) -> COLORREF {
        COLORREF(self.r as u32 | (self.g as u32) << 8 | (self.b as u32) << 16)
    }
}

/// A GDI font request: face, size in points, weight (`DESIGN.md` section 4).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FontSpec {
    /// Font family name (`UI_FACE` or `DATA_FACE`); `dpi::Font` resolves the weight face.
    pub face: &'static str,
    /// Size in points (`px * 72 / 96`, so the pixel size of the type scale lands exactly).
    pub points: f32,
    /// `lfWeight`: 400, 500, or 600.
    pub weight: i32,
}

impl FontSpec {
    const fn new(face: &'static str, points: f32, weight: i32) -> Self {
        Self {
            face,
            points,
            weight,
        }
    }
}

const fn pad(l: i32, t: i32, r: i32, b: i32) -> Pad {
    Pad { l, t, r, b }
}

const fn all(v: i32) -> Pad {
    pad(v, v, v, v)
}

const fn size(w: i32, h: i32) -> Size {
    Size { w, h }
}

// ---------------------------------------------------------------------------------------------
// DESIGN.md section 3: color tokens
// ---------------------------------------------------------------------------------------------

/// Window background, sidebar, title bar.
pub const BG: Color = Color::hex(0x09090B);
/// Panels, text boxes, list boxes, outline button at rest.
pub const CARD: Color = Color::hex(0x111114);
/// Hover fill, selected sidebar item, progress track.
pub const HOVER: Color = Color::hex(0x18181B);
/// Every divider and outline.
pub const BORDER: Color = Color::hex(0x27272A);
/// Hovered outline, dialog outline, text selection.
pub const BORDER_STRONG: Color = Color::hex(0x3F3F46);
/// Primary text.
pub const TEXT: Color = Color::hex(0xFAFAFA);
/// Supporting text, inactive icons, focus ring.
pub const SECONDARY: Color = Color::hex(0xA1A1AA);
/// Captions, placeholders, disabled text.
pub const FAINT: Color = Color::hex(0x71717A);
/// Done, removed, cleared, up to date.
pub const SUCCESS: Color = Color::hex(0x4ADE80);
/// Skipped, cancelled, dry run, presence unclear.
pub const WARNING: Color = Color::hex(0xFBBF24);
/// Failed, error, destructive action text.
pub const DANGER: Color = Color::hex(0xF87171);
/// Running, loading, notices.
pub const INFO: Color = Color::hex(0x60A5FA);
/// Primary button hover (fixed one-off value).
pub const PRIMARY_HOVER: Color = Color::hex(0xE4E4E7);
/// Primary button pressed (fixed one-off value).
pub const PRIMARY_PRESSED: Color = Color::hex(0xD4D4D8);
/// Log and raw-report text well (fixed one-off value).
pub const TEXT_WELL: Color = Color::hex(0x0C0C0E);

// ---------------------------------------------------------------------------------------------
// ThemeColors.cs (1:1, same order), pointed at the tokens
// ---------------------------------------------------------------------------------------------

/// `ThemeColors.MainBackground`.
pub const MAIN_BACKGROUND: Color = BG;
/// `ThemeColors.SecondaryBackground`.
pub const SECONDARY_BACKGROUND: Color = CARD;
/// `ThemeColors.ContentBackground` (section header panel).
pub const CONTENT_BACKGROUND: Color = CARD;
/// `ThemeColors.SurfaceBackground` (content area around the header and text).
pub const SURFACE_BACKGROUND: Color = BG;
/// `ThemeColors.BorderSubtle`.
pub const BORDER_SUBTLE: Color = BORDER;
/// `ThemeColors.SidebarBackground`.
pub const SIDEBAR_BACKGROUND: Color = BG;
/// `ThemeColors.SidebarItemBackground` (a sidebar item has no fill at rest).
pub const SIDEBAR_ITEM_BACKGROUND: Color = BG;
/// `ThemeColors.SidebarItemHover`.
pub const SIDEBAR_ITEM_HOVER: Color = HOVER;
/// `ThemeColors.SidebarItemActive`.
pub const SIDEBAR_ITEM_ACTIVE: Color = HOVER;
/// `ThemeColors.SidebarItemText`.
pub const SIDEBAR_ITEM_TEXT: Color = SECONDARY;
/// `ThemeColors.SidebarItemActiveText`.
pub const SIDEBAR_ITEM_ACTIVE_TEXT: Color = TEXT;
/// `ThemeColors.SidebarHeaderText`.
pub const SIDEBAR_HEADER_TEXT: Color = TEXT;
/// `ThemeColors.MutedText`.
pub const MUTED_TEXT: Color = FAINT;
/// `ThemeColors.SuccessText`.
pub const SUCCESS_TEXT: Color = SUCCESS;
/// `ThemeColors.ButtonBackground` (outline button at rest).
pub const BUTTON_BACKGROUND: Color = CARD;
/// `ThemeColors.ButtonHover`.
pub const BUTTON_HOVER: Color = HOVER;
/// `ThemeColors.ButtonBorder`.
pub const BUTTON_BORDER: Color = BORDER;
/// `ThemeColors.PrimaryButton` (white fill).
pub const PRIMARY_BUTTON: Color = TEXT;
/// `ThemeColors.PrimaryButtonHover`.
pub const PRIMARY_BUTTON_HOVER: Color = PRIMARY_HOVER;
/// `ThemeColors.PrimaryButtonPressed`.
pub const PRIMARY_BUTTON_PRESSED: Color = PRIMARY_PRESSED;
/// `ThemeColors.DisabledButton` (disabled primary fill).
pub const DISABLED_BUTTON: Color = BORDER;
/// `ThemeColors.DisabledText`.
pub const DISABLED_TEXT: Color = FAINT;
/// `ThemeColors.DangerButton`: the destructive button is an outline button whose text is this.
pub const DANGER_BUTTON: Color = DANGER;
/// `ThemeColors.DangerButtonHover` (same hover fill as every outline button).
pub const DANGER_BUTTON_HOVER: Color = HOVER;
/// `ThemeColors.PrimaryText`.
pub const PRIMARY_TEXT: Color = TEXT;
/// `ThemeColors.SecondaryText`.
pub const SECONDARY_TEXT: Color = SECONDARY;
/// `ThemeColors.TextBoxBackground` (hardware sections, whitelist list).
pub const TEXT_BOX_BACKGROUND: Color = CARD;
/// `ThemeColors.TextBoxText`.
pub const TEXT_BOX_TEXT: Color = TEXT;
/// `ThemeColors.ButtonPanelBackground`.
pub const BUTTON_PANEL_BACKGROUND: Color = BG;
/// `ThemeColors.LoadingLabelBackground` (defined in C#, never applied).
pub const LOADING_LABEL_BACKGROUND: Color = CARD;
/// `ThemeColors.LoadingLabelText` (status: loading).
pub const LOADING_LABEL_TEXT: Color = INFO;

// ---------------------------------------------------------------------------------------------
// Colors used outside ThemeColors.cs
// ---------------------------------------------------------------------------------------------

/// Confirm dialog back color (`DeviceRemovalConfirmationForm.cs:44`).
pub const CONFIRM_BACKGROUND: Color = BG;
/// Confirm dialog message text (`Color.White`).
pub const CONFIRM_MESSAGE_TEXT: Color = TEXT;
/// Confirm dialog warning text (`Color.Orange`; status: warning).
pub const CONFIRM_WARNING_TEXT: Color = WARNING;
/// Old View window back color (`SectionedViewForm.cs:841`).
pub const OLD_VIEW_BACKGROUND: Color = BG;
/// Old View text box back color (`SectionedViewForm.cs:851`; raw-report text well).
pub const OLD_VIEW_TEXT_BACKGROUND: Color = TEXT_WELL;
/// Old View text box text color (`SectionedViewForm.cs:852`).
pub const OLD_VIEW_TEXT: Color = TEXT;
/// Device Cleaning and Log Cleaning output back color (log text well).
pub const CLEANER_OUTPUT_BACKGROUND: Color = TEXT_WELL;
/// Update window back color (`SystemColors.Control` in C#).
pub const UPDATE_BACKGROUND: Color = BG;
/// Update window label text (`SystemColors.ControlText` in C#).
pub const UPDATE_TEXT: Color = TEXT;
/// Whitelist list selected item back color (`SystemColors.Highlight` in C#).
pub const LIST_SELECTED_BACKGROUND: Color = BORDER_STRONG;
/// Whitelist list selected item text color (`SystemColors.HighlightText` in C#).
pub const LIST_SELECTED_TEXT: Color = TEXT;
/// Progress bar track.
pub const PROGRESS_TRACK: Color = HOVER;
/// Progress bar fill.
pub const PROGRESS_FILL: Color = TEXT;
/// Focus ring color.
pub const FOCUS_RING: Color = SECONDARY;
/// Marquee block advance per step (device pixels).
pub const MARQUEE_STEP_PX: i32 = 4;
/// Marquee step period: the progress control's `PBM_SETMARQUEE` timer.
pub const MARQUEE_STEP_MS: i32 = 30;

// ---------------------------------------------------------------------------------------------
// DESIGN.md section 5: shape
// ---------------------------------------------------------------------------------------------

/// Button corner radius.
pub const BUTTON_RADIUS: i32 = 6;
/// Single-line inputs use the button radius.
pub const INPUT_RADIUS: i32 = BUTTON_RADIUS;
/// Focus ring corner radius.
pub const FOCUS_RING_RADIUS: i32 = 8;
/// Focus ring distance outside the control.
pub const FOCUS_RING_OFFSET: i32 = 3;
/// Focus ring and border stroke width (device pixels, never scaled).
pub const STROKE: i32 = 1;

// ---------------------------------------------------------------------------------------------
// DESIGN.md section 4: type
// ---------------------------------------------------------------------------------------------

/// UI font family (embedded Inter; `dpi::Font` falls back to Segoe UI).
pub const UI_FACE: &str = "Inter";
/// Data font family.
pub const DATA_FACE: &str = "Consolas";
/// Font weight 400.
pub const REGULAR: i32 = 400;
/// Font weight 500.
pub const MEDIUM: i32 = 500;
/// Font weight 600.
pub const SEMIBOLD: i32 = 600;

/// Points for a 96-DPI pixel size (11 px = 8.25 pt and so on).
const fn px(pixels: f32) -> f32 {
    pixels * 0.75
}

/// 11 px 600: small captions, section meta.
pub const CAPTION_FONT: FontSpec = FontSpec::new(UI_FACE, px(11.0), SEMIBOLD);
/// 12 px 400: secondary lines, sidebar subtitles, hints.
pub const SMALL_FONT: FontSpec = FontSpec::new(UI_FACE, px(12.0), REGULAR);
/// 13 px 400: body, inputs.
pub const BODY_FONT: FontSpec = FontSpec::new(UI_FACE, px(13.0), REGULAR);
/// 13 px 500: buttons.
pub const BUTTON_FONT: FontSpec = FontSpec::new(UI_FACE, px(13.0), MEDIUM);
/// 15 px 600: section titles.
pub const TITLE_FONT: FontSpec = FontSpec::new(UI_FACE, px(15.0), SEMIBOLD);
/// 18 px 600: window titles inside content.
pub const HEADING_FONT: FontSpec = FontSpec::new(UI_FACE, px(18.0), SEMIBOLD);

/// WinForms `Control.DefaultFont` for controls without an explicit font (body).
pub const DEFAULT_FONT: FontSpec = BODY_FONT;
/// Main window section title label (`Segoe UI Semibold` 12.5 in C#).
pub const SECTION_TITLE_FONT: FontSpec = HEADING_FONT;
/// Main window `Section {i} of {n}` label.
pub const SECTION_META_FONT: FontSpec = CAPTION_FONT;
/// Main window content text box (data).
pub const CONTENT_FONT: FontSpec = FontSpec::new(DATA_FACE, 10.0, REGULAR);
/// Main window loading overlay label.
pub const LOADING_FONT: FontSpec = BODY_FONT;
/// Sidebar `Hardware Sections` title (`Segoe UI Semibold` 13 in C#).
pub const SIDEBAR_TITLE_FONT: FontSpec = TITLE_FONT;
/// Sidebar `{n} sections` subtitle.
pub const SIDEBAR_SUBTITLE_FONT: FontSpec = SMALL_FONT;
/// Sidebar section buttons (`Segoe UI` 9.75 = 13 px in C#).
pub const SECTION_BUTTON_FONT: FontSpec = BODY_FONT;
/// Device Cleaning and Log Cleaning output boxes (data).
pub const CLEANER_OUTPUT_FONT: FontSpec = FontSpec::new(DATA_FACE, 9.75, REGULAR);
/// Whitelist window header label.
pub const WHITELIST_HEADER_FONT: FontSpec = BODY_FONT;
/// Whitelist checked list box (data).
pub const WHITELIST_LIST_FONT: FontSpec = FontSpec::new(DATA_FACE, 9.75, REGULAR);
/// Confirm dialog message label.
pub const CONFIRM_MESSAGE_FONT: FontSpec = BODY_FONT;
/// Confirm dialog warning label (`Segoe UI` 8.5 italic in C#; the type scale has no italic).
pub const CONFIRM_WARNING_FONT: FontSpec = SMALL_FONT;
/// Old View text box (data).
pub const OLD_VIEW_FONT: FontSpec = FontSpec::new(DATA_FACE, 9.0, REGULAR);
/// Update window step label (the window's heading).
pub const UPDATE_LABEL_FONT: FontSpec = TITLE_FONT;
/// Update window status line (`Segoe UI` 8 in C#).
pub const UPDATE_DETAIL_FONT: FontSpec = SMALL_FONT;
/// Main window loading title.
pub const LOADING_TITLE_FONT: FontSpec = TITLE_FONT;
/// Main window loading progress line (`Collected {n} of {total} sections`).
pub const LOADING_PROGRESS_FONT: FontSpec = SMALL_FONT;
/// Message box text.
pub const MSGBOX_FONT: FontSpec = BODY_FONT;

// ---------------------------------------------------------------------------------------------
// DESIGN.md section 14: icons (Segoe Fluent Icons, Segoe MDL2 Assets on Windows 10)
// ---------------------------------------------------------------------------------------------

/// Icon size in buttons and the sidebar (logical px).
pub const ICON_PX: i32 = 16;
/// Icon size in message boxes (logical px).
pub const MSGBOX_ICON_PX: i32 = 24;
/// Gap between a button icon and its text.
pub const ICON_GAP: i32 = 8;
/// Point size of the probe font that resolves the icon face once (`dpi::icon_face`).
pub const ICON_PROBE_POINTS: f32 = 12.0;

/// The icon font at `px` logical pixels; the face is resolved once (`dpi::icon_face`).
pub fn icon_font(pixels: i32) -> FontSpec {
    FontSpec::new(super::dpi::icon_face(), px(pixels as f32), REGULAR)
}

/// Glyphs (`Segoe Fluent Icons` code points; the same codes exist in `Segoe MDL2 Assets`).
pub mod glyph {
    /// Toggle on-state.
    pub const CHECK_MARK: char = '\u{E73E}';
    /// Mask IDs off-state.
    pub const HIDE: char = '\u{ED1A}';
    /// Compare Exports.
    pub const SWITCH: char = '\u{E8AB}';
    /// Previous find match.
    pub const CHEVRON_UP: char = '\u{E70E}';
    /// Next find match.
    pub const CHEVRON_DOWN: char = '\u{E70D}';
    /// Close find.
    pub const CANCEL: char = '\u{E711}';
    /// `HardDrive`: DISK DRIVES.
    pub const HARD_DRIVE: char = '\u{EDA2}';
    /// `Component`: MOTHERBOARD.
    pub const COMPONENT: char = '\u{E950}';
    /// `PC1`: CHASSIS.
    pub const PC: char = '\u{E977}';
    /// `CommandPrompt`: (SM)BIOS.
    pub const COMMAND_PROMPT: char = '\u{E756}';
    /// `Info`: SYSTEM INFORMATION.
    pub const INFO: char = '\u{E946}';
    /// `RAM`: RAM MODULES.
    pub const RAM: char = '\u{EEA0}';
    /// `CPU`: CPU.
    pub const CPU: char = '\u{EEA1}';
    /// `Lock`: TPM MODULES.
    pub const LOCK: char = '\u{E72E}';
    /// `USB`: USB DEVICES.
    pub const USB: char = '\u{E88E}';
    /// `Game`: GPU INFO.
    pub const GAME: char = '\u{E7FC}';
    /// `TVMonitor`: MONITOR INFORMATION.
    pub const MONITOR: char = '\u{E7F4}';
    /// `Ethernet`: NETWORK ADAPTERS.
    pub const ETHERNET: char = '\u{E839}';
    /// `Bluetooth`: BLUETOOTH ADAPTERS.
    pub const BLUETOOTH: char = '\u{E702}';
    /// `Network`: ARP INFO/CACHE.
    pub const NETWORK: char = '\u{E968}';
    /// `List`: any other section.
    pub const LIST: char = '\u{EA37}';
    /// `Refresh`: Refresh.
    pub const REFRESH: char = '\u{E72C}';
    /// `Save`: Export.
    pub const SAVE: char = '\u{E74E}';
    /// `Broom`: Clean Devices.
    pub const BROOM: char = '\u{EA99}';
    /// `Delete`: Clean Logs.
    pub const DELETE: char = '\u{E74D}';
    /// `Sync`: Updates / Checking.
    pub const SYNC: char = '\u{E895}';
    /// `History`: Old View.
    pub const HISTORY: char = '\u{E81C}';
    /// `Copy`: Copy section.
    pub const COPY: char = '\u{E8C8}';
    /// `StatusCircleRing`: the circle under every message box symbol (the symbols are drawn
    /// stacked on it).
    pub const STATUS_RING: char = '\u{F138}';
    /// `StatusCircleInfo`: information box.
    pub const STATUS_INFO: char = '\u{F13F}';
    /// `StatusCircleExclamation`: warning box.
    pub const STATUS_WARNING: char = '\u{F13C}';
    /// `StatusCircleErrorX`: error box.
    pub const STATUS_ERROR: char = '\u{F13D}';
    /// `StatusCircleQuestionMark`: question box.
    pub const STATUS_QUESTION: char = '\u{F142}';
}

// ---------------------------------------------------------------------------------------------
// WinForms framework defaults (used when C# does not set a value)
// ---------------------------------------------------------------------------------------------

/// `Control.DefaultMargin` (3 on every side).
pub const DEFAULT_MARGIN: Pad = all(3);
/// `Label.DefaultMargin` (3, 0, 3, 0).
pub const LABEL_DEFAULT_MARGIN: Pad = pad(3, 0, 3, 0);
/// Padding 0.
pub const NO_PAD: Pad = all(0);
/// `Panel`, `FlowLayoutPanel`, `TableLayoutPanel` default size.
pub const PANEL_DEFAULT_SIZE: Size = size(200, 100);
/// `Button` default size.
pub const BUTTON_DEFAULT_SIZE: Size = size(75, 23);
/// `Label` default size.
pub const LABEL_DEFAULT_SIZE: Size = size(100, 23);
/// `TextBox` default size.
pub const TEXT_BOX_DEFAULT_SIZE: Size = size(100, 23);
/// `ListBox` default size.
pub const LIST_BOX_DEFAULT_SIZE: Size = size(120, 96);
/// `ProgressBar` default size.
pub const PROGRESS_BAR_DEFAULT_SIZE: Size = size(100, 23);
/// `ListBox` item start position and border height (`ListBox.cs:100-105`).
pub const LIST_ITEM_START: i32 = 1;
/// `ListBox` per-side item border height (`ListBox.cs:105`).
pub const LIST_ITEM_BORDER: i32 = 1;
/// `CheckedListBox` check glyph size at 96 DPI.
pub const CHECK_GLYPH: i32 = 13;
/// Scroll step for one scroll-bar arrow click in an AutoScroll panel.
pub const SCROLL_LINE: i32 = 5;

// ---------------------------------------------------------------------------------------------
// Shared button style (Buttons.cs, CreateModernButton, CreateActionButton)
// ---------------------------------------------------------------------------------------------

/// `Buttons.ApplyStyle` padding (overrides every constructor padding).
pub const SHARED_BUTTON_PADDING: Pad = pad(10, 5, 10, 5);
/// `FlatAppearance.BorderSize = 1` (kept in the preferred-size math of every button).
pub const BUTTON_BORDER_SIZE: i32 = 1;

// ---------------------------------------------------------------------------------------------
// Main window (SectionedViewForm.cs)
// ---------------------------------------------------------------------------------------------

/// `DefaultWindowWidth` x `DefaultWindowHeight` (client).
pub const MAIN_CLIENT_SIZE: Size = size(1040, 800);
/// `MinimumWindowWidth` x `MinimumWindowHeight` (outer).
pub const MAIN_MIN_SIZE: Size = size(900, 750);
/// `SidebarMinWidth` (scaled by DPI, owner Q1).
pub const SIDEBAR_MIN_WIDTH: i32 = 240;
/// `SidebarMaxWidth` (scaled by DPI, owner Q1).
pub const SIDEBAR_MAX_WIDTH: i32 = 360;
/// `SidebarWidthPercentage`.
pub const SIDEBAR_WIDTH_PERCENT: i32 = 28;
/// Sidebar panel padding.
pub const SIDEBAR_PADDING: Pad = pad(0, 8, 0, 8);
/// Sidebar item width = sidebar - margins - padding - scroll bar (once) - this inset (scaled).
pub const SIDEBAR_ITEM_INSET: i32 = 12;
/// Lower bound of the sidebar item width (scaled).
pub const SIDEBAR_ITEM_MIN_WIDTH: i32 = 160;
/// Sidebar title label height.
pub const SIDEBAR_TITLE_HEIGHT: i32 = 34;
/// Sidebar title label margin.
pub const SIDEBAR_TITLE_MARGIN: Pad = pad(12, 0, 12, 0);
/// Sidebar subtitle label height.
pub const SIDEBAR_SUBTITLE_HEIGHT: i32 = 20;
/// Sidebar subtitle label margin.
pub const SIDEBAR_SUBTITLE_MARGIN: Pad = pad(12, 0, 12, 8);
/// Sidebar section button height (tier A).
pub const SECTION_BUTTON_HEIGHT: i32 = 42;
/// Sidebar section button padding.
pub const SECTION_BUTTON_PADDING: Pad = pad(12, 0, 0, 0);
/// Fixed sidebar tools row height.
pub const TOOLS_ROW_HEIGHT: i32 = 28;
/// Gap between the two tools rows.
pub const TOOLS_ROW_GAP: i32 = 2;
/// Gap between the two compare outline actions (C2b).
pub const TOOLS_SPLIT_GAP: i32 = 4;
/// Minimum horizontal text inset for the compare pair.
pub const COMPARE_PAIR_PADDING: i32 = 8;
/// Divider inset and gaps; its stroke stays one device pixel.
pub const TOOLS_DIVIDER_MARGIN: Pad = pad(12, 8, 12, 8);
/// Legacy sidebar trailing-check inset (kept for public API compatibility).
pub const TOOLS_CHECK_INSET: i32 = 12;
/// Size of the check/X badge on the sidebar toggle's leading icon.
pub const TOOLS_BADGE_PX: i32 = 8;
/// Badge extends this far beyond the icon's lower-right corner, into its existing gap.
pub const TOOLS_BADGE_OFFSET: i32 = 2;
/// Tools use the section item inset.
pub const TOOLS_ROW_PADDING: Pad = SECTION_BUTTON_PADDING;
/// Tools row outer margins match the section items, without a bottom gap.
pub const TOOLS_ROW_MARGIN: Pad = pad(12, 0, 12, 0);
/// Tools block height excluding its one-device-pixel divider (75 total at 96 DPI).
pub const TOOLS_BLOCK_CONTENT: i32 =
    2 * TOOLS_ROW_HEIGHT + TOOLS_ROW_GAP + TOOLS_DIVIDER_MARGIN.t + TOOLS_DIVIDER_MARGIN.b;
/// Find bar row height.
pub const FIND_BAR_HEIGHT: i32 = 28;
/// Room a focus ring needs outside a control: its offset plus its stroke.
pub const FIND_RING_ROOM: i32 = FOCUS_RING_OFFSET + STROKE;
/// The bar panel pads its children by the ring room, so their focus rings are not clipped by
/// the panel's own window.
pub const FIND_BAR_PADDING: Pad = all(FIND_RING_ROOM);
/// The bar overhangs its row by the ring room on three sides, so the input stays flush with
/// the well and the row still costs 28 + 8 px; the gap below is the 8 px minus the room.
pub const FIND_BAR_MARGIN: Pad = pad(
    -FIND_RING_ROOM,
    -FIND_RING_ROOM,
    -FIND_RING_ROOM,
    FIND_GAP - FIND_RING_ROOM,
);
/// The main content pane keeps the 12 px around the header and the well, but the left/right
/// ring room belongs to the content table (a panel clamps a child's negative margin), so the
/// find bar's overhang stays inside the table's window.
pub const CONTENT_PANE_PADDING: Pad = pad(
    CONTENT_PADDING.l - FIND_RING_ROOM,
    CONTENT_PADDING.t,
    CONTENT_PADDING.r - FIND_RING_ROOM,
    CONTENT_PADDING.b,
);
/// See `CONTENT_PANE_PADDING`.
pub const CONTENT_TABLE_PADDING: Pad = pad(FIND_RING_ROOM, 0, FIND_RING_ROOM, 0);
/// Minimum input width.
pub const FIND_EDIT_MIN_WIDTH: i32 = 160;
/// Minimum match-count width.
pub const FIND_COUNT_MIN_WIDTH: i32 = 72;
/// Icon-only find buttons.
pub const FIND_BUTTON_SIZE: Size = size(28, 28);
/// Find control gaps.
pub const FIND_GAP: i32 = 8;
/// Gap between previous and next.
pub const FIND_PAIR_GAP: i32 = 4;
/// Native input length limit, UTF-16 units.
pub const FIND_QUERY_MAX: usize = 256;
/// Input font.
pub const FIND_EDIT_FONT: FontSpec = BODY_FONT;
/// Find count font.
pub const FIND_COUNT_FONT: FontSpec = SMALL_FONT;
/// Mask toggle minimum size.
pub const MASK_BUTTON_SIZE: Size = size(96, 28);
/// Compare window default outer size.
pub const COMPARE_SIZE: Size = size(1000, 700);
/// Compare window minimum outer size.
pub const COMPARE_MIN_SIZE: Size = size(640, 400);
/// Compare header's Before/After label column.
pub const COMPARE_LABEL_WIDTH: i32 = 56;
/// Compare header's file-name row height.
pub const COMPARE_FILE_HEIGHT: i32 = 20;
/// C2c table metrics; strokes remain device pixels.
pub const COMPARE_ROW_HEIGHT: i32 = 24;
pub const COMPARE_HEADER_HEIGHT: i32 = 24;
pub const COMPARE_FILTER_HEIGHT: i32 = 28;
pub const COMPARE_STATUS_WIDTH: i32 = 88;
pub const COMPARE_FIELD_WIDTH: i32 = 200;
pub const COMPARE_FIELD_MIN_WIDTH: i32 = 140;
pub const COMPARE_VALUE_MIN_WIDTH: i32 = 150;
pub const COMPARE_CELL_PADDING: i32 = 8;
pub const COMPARE_VERDICT_BAR: i32 = 2;
pub const COMPARE_SECTION_INSET: i32 = 8;
pub const COMPARE_DEVICE_INSET: i32 = 24;
pub const COMPARE_TEXT_INSET: i32 = 32;
pub const COMPARE_FIELD_INSET: i32 = 48;
/// Sidebar section button margin (tier A: gap 4).
pub const SECTION_BUTTON_MARGIN: Pad = pad(12, 0, 12, 4);
/// Width of the accent bar on the active sidebar item.
pub const SIDEBAR_ACCENT_WIDTH: i32 = 2;
/// Vertical inset of the accent bar (top and bottom).
pub const SIDEBAR_ACCENT_INSET: i32 = 4;
/// Corner radius of the accent bar (device pixels, never scaled; `DESIGN.md` 11.7).
pub const SIDEBAR_ACCENT_RADIUS: i32 = 1;
/// Sidebar tiers (`DESIGN.md` section 11): item height, gap below, title height and gap,
/// subtitle shown. The first tier whose content fits the sidebar is used.
pub struct SidebarTier {
    /// Section button height.
    pub item: i32,
    /// Gap below each section button.
    pub gap: i32,
    /// Title label height.
    pub title: i32,
    /// Gap below the title when the subtitle is hidden.
    pub title_gap: i32,
    /// Whether the `{n} sections` subtitle is shown.
    pub subtitle: bool,
}
/// Tier A (default), B, C, D in order.
pub const SIDEBAR_TIERS: [SidebarTier; 4] = [
    SidebarTier {
        item: 42,
        gap: 4,
        title: 34,
        title_gap: 0,
        subtitle: true,
    },
    SidebarTier {
        item: 36,
        gap: 4,
        title: 34,
        title_gap: 0,
        subtitle: true,
    },
    SidebarTier {
        item: 32,
        gap: 3,
        title: 34,
        title_gap: 6,
        subtitle: false,
    },
    SidebarTier {
        item: 28,
        gap: 2,
        title: 28,
        title_gap: 4,
        subtitle: false,
    },
];
/// Content panel padding.
pub const CONTENT_PADDING: Pad = all(12);
/// Section header card padding.
pub const HEADER_PADDING: Pad = pad(12, 8, 12, 8);
/// Section header card margin (the gap to the text well).
pub const HEADER_MARGIN: Pad = pad(0, 0, 0, 8);
/// Section header card corner radius.
pub const HEADER_RADIUS: i32 = 8;
/// Section title label height.
pub const SECTION_TITLE_HEIGHT: i32 = 26;
/// Section meta label height.
pub const SECTION_META_HEIGHT: i32 = 18;
/// `Copy` button minimum size in the section header (auto-sized to its icon and text).
pub const COPY_BUTTON_SIZE: Size = size(72, 28);
/// Footer button panel padding.
pub const FOOTER_PADDING: Pad = pad(12, 8, 12, 8);
/// Footer button minimum size.
pub const FOOTER_BUTTON_MIN: Size = size(120, 34);
/// Footer button margin (one row); the bottom margin returns only when the footer wraps.
pub const FOOTER_BUTTON_MARGIN: Pad = pad(0, 0, 8, 0);
/// Gap between wrapped footer rows.
pub const FOOTER_ROW_GAP: i32 = 8;
/// Parent-painted inner padding on all four sides of every well and checked list.
pub const EDIT_INNER_MARGIN: i32 = 10;
/// Loading indicator diameter.
pub const SPINNER_SIZE: i32 = 32;
/// Loading indicator stroke width.
pub const SPINNER_STROKE: i32 = 3;
/// Loading indicator: one turn in milliseconds.
pub const SPINNER_TURN_MS: i32 = 1200;
/// Loading indicator repaint period.
pub const SPINNER_STEP_MS: u32 = 33;
/// Gap below the loading indicator.
pub const SPINNER_MARGIN: Pad = pad(0, 0, 0, 16);
/// Gap below the loading title.
pub const LOADING_TITLE_MARGIN: Pad = pad(0, 0, 0, 4);

// ---------------------------------------------------------------------------------------------
// Old View (inline form in SectionedViewForm.cs:836-858)
// ---------------------------------------------------------------------------------------------

/// Old View outer size.
pub const OLD_VIEW_SIZE: Size = size(1000, 700);
/// Old View minimum outer size (C# has none).
pub const OLD_VIEW_MIN_SIZE: Size = size(640, 400);

// ---------------------------------------------------------------------------------------------
// Device Cleaning (CleanDevicesForm.cs)
// ---------------------------------------------------------------------------------------------

/// `DefaultWidth` x `DefaultHeight` (client).
pub const CLEAN_DEVICES_CLIENT_SIZE: Size = size(920, 640);
/// `MinimumWidth` x `MinimumHeight` (outer).
pub const CLEAN_DEVICES_MIN_SIZE: Size = size(760, 500);
/// Output panel padding (cleaner, whitelist and Old View windows).
pub const OUTPUT_PANEL_PADDING: Pad = all(12);
/// Footer row height (cleaner and whitelist windows): padding 12 + button 36 + padding 12.
pub const ACTION_ROW_HEIGHT: i32 = 60;
/// Footer flow panel padding (cleaner and whitelist windows).
pub const ACTION_PANEL_PADDING: Pad = all(12);
/// `CreateActionButton` minimum size (Device Cleaning, whitelist).
pub const ACTION_BUTTON_MIN: Size = size(130, 34);
/// `CreateActionButton` margin (Device Cleaning, Log Cleaning, whitelist).
pub const ACTION_BUTTON_MARGIN: Pad = pad(0, 0, 8, 0);

// ---------------------------------------------------------------------------------------------
// Log Cleaning (CleanLogsForm.cs)
// ---------------------------------------------------------------------------------------------

/// `DefaultWidth` x `DefaultHeight` (client).
pub const CLEAN_LOGS_CLIENT_SIZE: Size = size(760, 460);
/// `MinimumWidth` x `MinimumHeight` (outer).
pub const CLEAN_LOGS_MIN_SIZE: Size = size(620, 380);
/// Close button minimum size.
pub const CLEAN_LOGS_CLOSE_MIN: Size = size(110, 34);

// ---------------------------------------------------------------------------------------------
// Whitelist (WhitelistDevicesForm.cs)
// ---------------------------------------------------------------------------------------------

/// `DefaultWidth` x `DefaultHeight` (client).
pub const WHITELIST_CLIENT_SIZE: Size = size(920, 640);
/// `MinimumWidth` x `MinimumHeight` (outer).
pub const WHITELIST_MIN_SIZE: Size = size(720, 520);
/// Header label margin.
pub const WHITELIST_HEADER_MARGIN: Pad = pad(12, 12, 12, 4);

// ---------------------------------------------------------------------------------------------
// Confirm Device Removal (DeviceRemovalConfirmationForm.cs)
// ---------------------------------------------------------------------------------------------

/// `DialogWidth` x `DialogHeight` (client).
pub const CONFIRM_CLIENT_SIZE: Size = size(520, 210);
/// `MinimumDialogWidth` x `MinimumDialogHeight` (outer).
pub const CONFIRM_MIN_SIZE: Size = size(460, 190);
/// Root table padding.
pub const CONFIRM_PADDING: Pad = all(16);
/// Message label margin.
pub const CONFIRM_MESSAGE_MARGIN: Pad = pad(0, 0, 0, 4);
/// Warning label margin.
pub const CONFIRM_WARNING_MARGIN: Pad = pad(0, 0, 0, 10);
/// Button padding.
pub const CONFIRM_BUTTON_PADDING: Pad = pad(12, 4, 12, 4);
/// Button margin.
pub const CONFIRM_BUTTON_MARGIN: Pad = pad(0, 0, 8, 8);
/// Button minimum height.
pub const CONFIRM_BUTTON_HEIGHT: i32 = 34;
/// `Yes (Autoclose)` minimum width.
pub const CONFIRM_PRIMARY_MIN_WIDTH: i32 = 150;
/// `Yes` and `No` minimum width.
pub const CONFIRM_SECONDARY_MIN_WIDTH: i32 = 80;
/// Label wrap width = max(this, client width - `CONFIRM_LABEL_WRAP_INSET`).
pub const CONFIRM_LABEL_WRAP_MIN: i32 = 220;
/// See `CONFIRM_LABEL_WRAP_MIN`.
pub const CONFIRM_LABEL_WRAP_INSET: i32 = 40;

// ---------------------------------------------------------------------------------------------
// Update progress (inline form in AutoUpdateService.cs:148-182)
// ---------------------------------------------------------------------------------------------

/// Update window outer size.
pub const UPDATE_SIZE: Size = size(400, 150);
/// Update window padding.
pub const UPDATE_PADDING: Pad = all(16);
/// App icon size in the update window.
pub const UPDATE_ICON_SIZE: i32 = 32;
/// App icon margin (the gap to the text column).
pub const UPDATE_ICON_MARGIN: Pad = pad(0, 0, 12, 0);
/// Step label height.
pub const UPDATE_LABEL_HEIGHT: i32 = 20;
/// Step label margin.
pub const UPDATE_LABEL_MARGIN: Pad = pad(0, 0, 0, 2);
/// Status line height.
pub const UPDATE_DETAIL_HEIGHT: i32 = 18;
/// Progress bar height (a pill: radius = half the height).
pub const PROGRESS_BAR_HEIGHT: i32 = 8;
/// Progress bar margin (the gap above it).
pub const UPDATE_BAR_MARGIN: Pad = pad(0, 12, 0, 0);

// ---------------------------------------------------------------------------------------------
// Message box (themed `MessageBox.Show`, DESIGN.md section 15)
// ---------------------------------------------------------------------------------------------

/// Message box padding.
pub const MSGBOX_PADDING: Pad = all(20);
/// Icon margin (the gap to the text).
pub const MSGBOX_ICON_MARGIN: Pad = pad(0, 0, 14, 0);
/// Longest text line before wrapping (logical px).
pub const MSGBOX_TEXT_MAX_WIDTH: i32 = 440;
/// Minimum client width.
pub const MSGBOX_MIN_WIDTH: i32 = 320;
/// Width slack added to the measured wrap, so the unscaled width never rounds below it.
pub const MSGBOX_WIDTH_SLACK: i32 = 6;
/// Height slack added to the measured body, for the same rounding reason.
pub const MSGBOX_HEIGHT_SLACK: i32 = 4;
/// Tallest wrapped text body (logical px). Longer text goes into a scrolling well of this
/// height, so the box fits every work area of `DESIGN.md` 11 (client 106 + body at 96 DPI).
pub const MSGBOX_BODY_MAX_HEIGHT: i32 = 360;
/// Gap between the text and the button row.
pub const MSGBOX_BUTTON_ROW_MARGIN: Pad = pad(0, 20, 0, 0);
/// Button minimum width.
pub const MSGBOX_BUTTON_MIN_WIDTH: i32 = 88;
/// Button height.
pub const MSGBOX_BUTTON_HEIGHT: i32 = 34;
/// Button margin: 4 on every side (8 between buttons, room for the focus ring around them).
pub const MSGBOX_BUTTON_MARGIN: Pad = all(4);
