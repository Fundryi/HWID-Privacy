//! Owned by WP-10a: every color, font, size, padding and margin of the C# forms (IMPROVEMENTS A8).
//!
//! Names follow `UI/Components/ThemeColors.cs` and the C# form constants. Sizes are logical
//! pixels at 96 DPI; `dpi::scale` converts them. Form code uses these names, never literals.

use super::layout::{Pad, Point, Size};
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

    /// Returns the GDI `COLORREF` (0x00BBGGRR).
    pub const fn colorref(self) -> COLORREF {
        COLORREF(self.r as u32 | (self.g as u32) << 8 | (self.b as u32) << 16)
    }
}

/// A WinForms `Font` constructor call: face, size in points, style.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FontSpec {
    /// Font family name passed to GDI as `lfFaceName`.
    pub face: &'static str,
    /// Size in points (WinForms `GraphicsUnit.Point`).
    pub points: f32,
    /// `FontStyle.Bold`.
    pub bold: bool,
    /// `FontStyle.Italic`.
    pub italic: bool,
}

impl FontSpec {
    const fn new(face: &'static str, points: f32, bold: bool, italic: bool) -> Self {
        Self {
            face,
            points,
            bold,
            italic,
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
// ThemeColors.cs (1:1, same order)
// ---------------------------------------------------------------------------------------------

/// `ThemeColors.MainBackground`.
pub const MAIN_BACKGROUND: Color = Color::rgb(30, 30, 30);
/// `ThemeColors.SecondaryBackground`.
pub const SECONDARY_BACKGROUND: Color = Color::rgb(35, 35, 35);
/// `ThemeColors.ContentBackground`.
pub const CONTENT_BACKGROUND: Color = Color::rgb(45, 45, 45);
/// `ThemeColors.SurfaceBackground`.
pub const SURFACE_BACKGROUND: Color = Color::rgb(26, 26, 26);
/// `ThemeColors.BorderSubtle`.
pub const BORDER_SUBTLE: Color = Color::rgb(58, 58, 62);
/// `ThemeColors.SidebarBackground`.
pub const SIDEBAR_BACKGROUND: Color = Color::rgb(34, 34, 36);
/// `ThemeColors.SidebarItemBackground`.
pub const SIDEBAR_ITEM_BACKGROUND: Color = Color::rgb(46, 46, 50);
/// `ThemeColors.SidebarItemHover`.
pub const SIDEBAR_ITEM_HOVER: Color = Color::rgb(60, 60, 66);
/// `ThemeColors.SidebarItemActive`.
pub const SIDEBAR_ITEM_ACTIVE: Color = Color::rgb(0, 120, 215);
/// `ThemeColors.SidebarItemText`.
pub const SIDEBAR_ITEM_TEXT: Color = Color::rgb(210, 210, 210);
/// `ThemeColors.SidebarItemActiveText` (`Color.White`).
pub const SIDEBAR_ITEM_ACTIVE_TEXT: Color = WHITE;
/// `ThemeColors.SidebarHeaderText`.
pub const SIDEBAR_HEADER_TEXT: Color = Color::rgb(235, 235, 235);
/// `ThemeColors.MutedText`.
pub const MUTED_TEXT: Color = Color::rgb(165, 165, 170);
/// `ThemeColors.SuccessText`.
pub const SUCCESS_TEXT: Color = Color::rgb(125, 205, 125);
/// `ThemeColors.ButtonBackground`.
pub const BUTTON_BACKGROUND: Color = Color::rgb(45, 45, 48);
/// `ThemeColors.ButtonHover`.
pub const BUTTON_HOVER: Color = Color::rgb(62, 62, 66);
/// `ThemeColors.ButtonBorder`.
pub const BUTTON_BORDER: Color = Color::rgb(80, 80, 83);
/// `ThemeColors.PrimaryButton`.
pub const PRIMARY_BUTTON: Color = Color::rgb(0, 120, 215);
/// `ThemeColors.PrimaryButtonHover`.
pub const PRIMARY_BUTTON_HOVER: Color = Color::rgb(0, 140, 230);
/// `ThemeColors.PrimaryButtonPressed`.
pub const PRIMARY_BUTTON_PRESSED: Color = Color::rgb(0, 102, 184);
/// `ThemeColors.DisabledButton`.
pub const DISABLED_BUTTON: Color = Color::rgb(74, 74, 76);
/// `ThemeColors.DisabledText` (set as ForeColor; WinForms flat buttons ignore it when painting).
pub const DISABLED_TEXT: Color = Color::rgb(160, 160, 160);
/// `ThemeColors.DangerButton`.
pub const DANGER_BUTTON: Color = Color::rgb(170, 55, 55);
/// `ThemeColors.DangerButtonHover`.
pub const DANGER_BUTTON_HOVER: Color = Color::rgb(190, 65, 65);
/// `ThemeColors.PrimaryText` (`Color.White`).
pub const PRIMARY_TEXT: Color = WHITE;
/// `ThemeColors.SecondaryText`.
pub const SECONDARY_TEXT: Color = Color::rgb(220, 220, 220);
/// `ThemeColors.TextBoxBackground`.
pub const TEXT_BOX_BACKGROUND: Color = Color::rgb(45, 45, 45);
/// `ThemeColors.TextBoxText`.
pub const TEXT_BOX_TEXT: Color = Color::rgb(220, 220, 220);
/// `ThemeColors.ButtonPanelBackground`.
pub const BUTTON_PANEL_BACKGROUND: Color = Color::rgb(35, 35, 35);
/// `ThemeColors.LoadingLabelBackground` (defined in C#, never applied).
pub const LOADING_LABEL_BACKGROUND: Color = Color::rgb(45, 45, 45);
/// `ThemeColors.LoadingLabelText`.
pub const LOADING_LABEL_TEXT: Color = Color::rgb(220, 220, 220);

// ---------------------------------------------------------------------------------------------
// Named System.Drawing colors and literals used outside ThemeColors.cs
// ---------------------------------------------------------------------------------------------

/// `Color.White`.
pub const WHITE: Color = Color::rgb(255, 255, 255);
/// `Color.Orange` (confirm dialog warning text).
pub const ORANGE: Color = Color::rgb(255, 165, 0);

/// Confirm dialog back color (`DeviceRemovalConfirmationForm.cs:44`).
pub const CONFIRM_BACKGROUND: Color = Color::rgb(45, 45, 48);
/// Confirm dialog primary button back (`:147`).
pub const CONFIRM_PRIMARY: Color = Color::rgb(0, 122, 204);
/// Confirm dialog primary button border (`:156`).
pub const CONFIRM_PRIMARY_BORDER: Color = Color::rgb(0, 150, 255);
/// Confirm dialog primary button hover (`:162`).
pub const CONFIRM_PRIMARY_HOVER: Color = Color::rgb(0, 140, 230);
/// Confirm dialog secondary button back (`:147`).
pub const CONFIRM_SECONDARY: Color = Color::rgb(60, 60, 63);
/// Confirm dialog secondary button border (`:157`).
pub const CONFIRM_SECONDARY_BORDER: Color = Color::rgb(80, 80, 83);
/// Confirm dialog secondary button hover (`:162`).
pub const CONFIRM_SECONDARY_HOVER: Color = Color::rgb(80, 80, 83);

/// Old View window back color (`SectionedViewForm.cs:841`).
pub const OLD_VIEW_BACKGROUND: Color = Color::rgb(32, 32, 32);
/// Old View text box back color (`SectionedViewForm.cs:851`).
pub const OLD_VIEW_TEXT_BACKGROUND: Color = Color::rgb(25, 25, 25);
/// Old View text box text color (`SectionedViewForm.cs:852`).
pub const OLD_VIEW_TEXT: Color = Color::rgb(220, 220, 220);

// ---------------------------------------------------------------------------------------------
// Fonts
// ---------------------------------------------------------------------------------------------

/// WinForms `Control.DefaultFont` (Segoe UI 9) for controls without an explicit font.
pub const DEFAULT_FONT: FontSpec = FontSpec::new("Segoe UI", 9.0, false, false);
/// `Buttons.ButtonFont` (Segoe UI 9), applied by `Buttons.ApplyStyle`.
pub const BUTTON_FONT: FontSpec = FontSpec::new("Segoe UI", 9.0, false, false);
/// Main window section title label.
pub const SECTION_TITLE_FONT: FontSpec = FontSpec::new("Segoe UI Semibold", 12.5, true, false);
/// Main window `Section {i} of {n}` label.
pub const SECTION_META_FONT: FontSpec = FontSpec::new("Segoe UI", 9.0, false, false);
/// Main window content text box.
pub const CONTENT_FONT: FontSpec = FontSpec::new("Consolas", 10.0, false, false);
/// Main window loading overlay label.
pub const LOADING_FONT: FontSpec = FontSpec::new("Segoe UI", 10.0, false, false);
/// Sidebar `Hardware Sections` title.
pub const SIDEBAR_TITLE_FONT: FontSpec = FontSpec::new("Segoe UI Semibold", 13.0, true, false);
/// Sidebar `{n} sections` subtitle.
pub const SIDEBAR_SUBTITLE_FONT: FontSpec = FontSpec::new("Segoe UI", 8.75, false, false);
/// Sidebar section buttons.
pub const SECTION_BUTTON_FONT: FontSpec = FontSpec::new("Segoe UI", 9.75, false, false);
/// Device Cleaning and Log Cleaning output boxes.
pub const CLEANER_OUTPUT_FONT: FontSpec = FontSpec::new("Consolas", 9.75, false, false);
/// Whitelist window header label.
pub const WHITELIST_HEADER_FONT: FontSpec = FontSpec::new("Segoe UI", 10.0, false, false);
/// Whitelist checked list box.
pub const WHITELIST_LIST_FONT: FontSpec = FontSpec::new("Consolas", 9.75, false, false);
/// Confirm dialog message label.
pub const CONFIRM_MESSAGE_FONT: FontSpec = FontSpec::new("Segoe UI", 10.0, false, false);
/// Confirm dialog warning label.
pub const CONFIRM_WARNING_FONT: FontSpec = FontSpec::new("Segoe UI", 8.5, false, true);
/// Confirm dialog `Yes (Autoclose)` button.
pub const CONFIRM_PRIMARY_FONT: FontSpec = FontSpec::new("Segoe UI", 8.5, true, false);
/// Confirm dialog `Yes` and `No` buttons.
pub const CONFIRM_SECONDARY_FONT: FontSpec = FontSpec::new("Segoe UI", 8.5, false, false);
/// Old View text box.
pub const OLD_VIEW_FONT: FontSpec = FontSpec::new("Consolas", 9.0, false, false);
/// Update window detail label.
pub const UPDATE_DETAIL_FONT: FontSpec = FontSpec::new("Segoe UI", 8.0, false, false);

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
/// `FlatAppearance.BorderSize = 1`.
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
/// Sidebar item width = client - padding - scroll bar - this inset.
pub const SIDEBAR_ITEM_INSET: i32 = 24;
/// Lower bound of the sidebar item width.
pub const SIDEBAR_ITEM_MIN_WIDTH: i32 = 160;
/// Sidebar title label height.
pub const SIDEBAR_TITLE_HEIGHT: i32 = 34;
/// Sidebar title label margin.
pub const SIDEBAR_TITLE_MARGIN: Pad = pad(12, 0, 12, 0);
/// Sidebar subtitle label height.
pub const SIDEBAR_SUBTITLE_HEIGHT: i32 = 20;
/// Sidebar subtitle label margin.
pub const SIDEBAR_SUBTITLE_MARGIN: Pad = pad(12, 0, 12, 10);
/// Sidebar section button height.
pub const SECTION_BUTTON_HEIGHT: i32 = 42;
/// Sidebar section button padding.
pub const SECTION_BUTTON_PADDING: Pad = pad(12, 0, 0, 0);
/// Sidebar section button margin.
pub const SECTION_BUTTON_MARGIN: Pad = pad(12, 0, 12, 5);
/// Content panel padding.
pub const CONTENT_PADDING: Pad = all(14);
/// Section header panel padding.
pub const HEADER_PADDING: Pad = pad(14, 10, 14, 9);
/// Section title label height.
pub const SECTION_TITLE_HEIGHT: i32 = 26;
/// Section meta label height.
pub const SECTION_META_HEIGHT: i32 = 18;
/// Divider row height.
pub const DIVIDER_HEIGHT: i32 = 1;
/// Footer button panel padding.
pub const FOOTER_PADDING: Pad = pad(12, 8, 12, 8);
/// Footer button minimum size.
pub const FOOTER_BUTTON_MIN: Size = size(120, 34);
/// Footer button margin.
pub const FOOTER_BUTTON_MARGIN: Pad = pad(0, 0, 8, 8);

// ---------------------------------------------------------------------------------------------
// Old View (inline form in SectionedViewForm.cs:836-858)
// ---------------------------------------------------------------------------------------------

/// Old View outer size.
pub const OLD_VIEW_SIZE: Size = size(1000, 700);

// ---------------------------------------------------------------------------------------------
// Device Cleaning (CleanDevicesForm.cs)
// ---------------------------------------------------------------------------------------------

/// `DefaultWidth` x `DefaultHeight` (client).
pub const CLEAN_DEVICES_CLIENT_SIZE: Size = size(920, 640);
/// `MinimumWidth` x `MinimumHeight` (outer).
pub const CLEAN_DEVICES_MIN_SIZE: Size = size(760, 500);
/// Output panel padding (cleaner and whitelist windows).
pub const OUTPUT_PANEL_PADDING: Pad = all(10);
/// Footer row height (cleaner and whitelist windows).
pub const ACTION_ROW_HEIGHT: i32 = 56;
/// Footer flow panel padding (cleaner and whitelist windows).
pub const ACTION_PANEL_PADDING: Pad = all(10);
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
/// Primary button border size.
pub const CONFIRM_PRIMARY_BORDER_SIZE: i32 = 2;
/// Secondary button border size.
pub const CONFIRM_SECONDARY_BORDER_SIZE: i32 = 1;
/// Label wrap width = max(this, client width - `CONFIRM_LABEL_WRAP_INSET`).
pub const CONFIRM_LABEL_WRAP_MIN: i32 = 220;
/// See `CONFIRM_LABEL_WRAP_MIN`.
pub const CONFIRM_LABEL_WRAP_INSET: i32 = 40;

// ---------------------------------------------------------------------------------------------
// Update progress (inline form in AutoUpdateService.cs:148-182)
// ---------------------------------------------------------------------------------------------

/// Update window outer size.
pub const UPDATE_SIZE: Size = size(400, 150);
/// Progress label location.
pub const UPDATE_LABEL_POS: Point = Point { x: 10, y: 20 };
/// Progress label and detail label size.
pub const UPDATE_LABEL_SIZE: Size = size(360, 20);
/// Progress bar location.
pub const UPDATE_BAR_POS: Point = Point { x: 10, y: 50 };
/// Progress bar size.
pub const UPDATE_BAR_SIZE: Size = size(360, 25);
/// Detail label location.
pub const UPDATE_DETAIL_POS: Point = Point { x: 10, y: 85 };
