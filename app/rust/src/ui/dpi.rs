//! Owned by WP-10a: DPI scaling rules, per-DPI fonts, and window size math (PerMonitorV2).
//!
//! Every rounding rule lives here so the 1.11 baseline can correct it in one place.

use super::layout::Size;
use super::theme::{self, FontSpec};
use crate::win::{self, wide::to_wide};
use std::sync::OnceLock;
use windows::Win32::{
    Foundation::{HWND, RECT},
    Graphics::Gdi::{
        AddFontMemResourceEx, CLIP_DEFAULT_PRECIS, CreateFontIndirectW, DEFAULT_CHARSET,
        DEFAULT_QUALITY, DeleteObject, FF_DONTCARE, HFONT, LOGFONTW, OUT_DEFAULT_PRECIS,
    },
    UI::{
        HiDpi::{AdjustWindowRectExForDpi, GetDpiForWindow, GetSystemMetricsForDpi},
        WindowsAndMessaging::{SYSTEM_METRICS_INDEX, WINDOW_EX_STYLE, WINDOW_STYLE},
    },
};

/// The embedded UI font files (`DESIGN.md` section 4; OFL license in `assets/fonts`).
const INTER: [(&str, &[u8]); 3] = [
    (
        "Inter-Regular",
        include_bytes!("../../assets/fonts/Inter-Regular.ttf"),
    ),
    (
        "Inter-Medium",
        include_bytes!("../../assets/fonts/Inter-Medium.ttf"),
    ),
    (
        "Inter-SemiBold",
        include_bytes!("../../assets/fonts/Inter-SemiBold.ttf"),
    ),
];

/// Registers the embedded Inter files with GDI once (process private); false = fall back to
/// Segoe UI. The memory fonts stay registered for the process lifetime on purpose.
pub fn load_fonts() -> bool {
    static LOADED: OnceLock<bool> = OnceLock::new();
    *LOADED.get_or_init(|| {
        let mut ok = true;
        for (name, bytes) in INTER {
            let mut count = 0u32;
            // SAFETY: GDI copies the static font bytes. Despite the binding's const pointer,
            // pcFonts is an output parameter; count is writable and lives through the call.
            let handle = unsafe {
                AddFontMemResourceEx(
                    bytes.as_ptr().cast(),
                    bytes.len() as u32,
                    None,
                    &raw mut count,
                )
            };
            if handle.is_invalid() || count == 0 {
                win::record(win::Error::last(name));
                ok = false;
            }
        }
        ok
    })
}

/// The GDI face for a spec: Inter's weights are separate one-style families, and the fallback
/// is an explicit substitution (GDI would map an unknown face to a default, not to Segoe UI).
fn face_for(spec: FontSpec) -> &'static str {
    if spec.face != theme::UI_FACE {
        return spec.face;
    }
    match (load_fonts(), spec.weight) {
        (true, theme::MEDIUM) => "Inter Medium",
        (true, theme::SEMIBOLD) => "Inter SemiBold",
        (true, _) => "Inter",
        (false, theme::SEMIBOLD) => "Segoe UI Semibold",
        (false, _) => "Segoe UI",
    }
}

/// The icon font face: `Segoe Fluent Icons` (Windows 11) or `Segoe MDL2 Assets` (Windows 10).
/// Resolved once by creating the font and reading the face GDI really selected, because GDI
/// substitutes a default font for an unknown face without an error.
pub fn icon_face() -> &'static str {
    static FACE: OnceLock<&'static str> = OnceLock::new();
    FACE.get_or_init(|| {
        const FLUENT: &str = "Segoe Fluent Icons";
        let spec = FontSpec {
            face: FLUENT,
            points: theme::ICON_PROBE_POINTS,
            weight: theme::REGULAR,
        };
        let Ok(font) = Font::new(spec, BASE_DPI) else {
            return "Segoe MDL2 Assets";
        };
        let selected = font.selected_face();
        if selected.eq_ignore_ascii_case(FLUENT) {
            FLUENT
        } else {
            "Segoe MDL2 Assets"
        }
    })
}

/// The DPI at which every theme constant is defined.
pub const BASE_DPI: u32 = 96;

/// Converts a device length back to 96-DPI logical pixels (the inverse of [`scale`]).
pub fn unscale(value: i32, dpi: u32) -> i32 {
    if dpi == BASE_DPI {
        return value;
    }
    (f64::from(value) * f64::from(BASE_DPI) / f64::from(dpi)).round_ties_even() as i32
}

/// Scales a 96-DPI length to `dpi`, rounding like WinForms `Math.Round` (ties to even).
pub fn scale(value: i32, dpi: u32) -> i32 {
    if dpi == BASE_DPI {
        return value;
    }
    (f64::from(value) * f64::from(dpi) / f64::from(BASE_DPI)).round_ties_even() as i32
}

/// Scales both parts of a size.
pub fn scale_size(value: Size, dpi: u32) -> Size {
    Size {
        w: scale(value.w, dpi),
        h: scale(value.h, dpi),
    }
}

/// GDI `lfHeight` for a point size: minus the em height in pixels, rounded half away from zero.
pub fn font_height(points: f32, dpi: u32) -> i32 {
    -((f64::from(points) * f64::from(dpi) / 72.0).round() as i32)
}

/// Returns the DPI of the monitor that holds `hwnd`, or 96 when the call fails.
pub fn window_dpi(hwnd: HWND) -> u32 {
    // SAFETY: GetDpiForWindow only reads window state; an invalid handle returns 0.
    let dpi = unsafe { GetDpiForWindow(hwnd) };
    if dpi == 0 { BASE_DPI } else { dpi }
}

/// The DPI of the monitor under the cursor: where `Form::create` opens an ownerless form
/// (it places the window there and reads its DPI). Measured the same way, through a hidden
/// throwaway window at the cursor, because `GetDpiForMonitor` lives in shcore, which the exe
/// does not import. Falls back to the system DPI when the probe cannot be created.
pub fn cursor_dpi() -> u32 {
    use windows::Win32::Foundation::POINT;
    use windows::Win32::UI::HiDpi::GetDpiForSystem;
    use windows::Win32::UI::WindowsAndMessaging::{
        CreateWindowExW, DestroyWindow, GetCursorPos, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_POPUP,
    };
    use windows::core::{PCWSTR, w};
    struct Probe(HWND);
    impl Drop for Probe {
        fn drop(&mut self) {
            // SAFETY: This guard owns the hidden window, created and destroyed on this thread.
            if let Err(e) = unsafe { DestroyWindow(self.0) } {
                win::record(win::Error::from_win("DestroyWindow", e));
            }
        }
    }
    let system_dpi = || {
        // SAFETY: Plain value query.
        let system = unsafe { GetDpiForSystem() };
        if system == 0 { BASE_DPI } else { system }
    };
    let mut pt = POINT::default();
    // SAFETY: Writable POINT lives through the query.
    if let Err(e) = unsafe { GetCursorPos(&mut pt) } {
        win::record(win::Error::from_win("GetCursorPos", e));
        return system_dpi();
    }
    // SAFETY: Static class name; no borrowed creation data or parent. No WS_VISIBLE and
    // explicit no-activation/tool-window styles keep the probe hidden and off the taskbar.
    let probe = unsafe {
        CreateWindowExW(
            WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW,
            w!("STATIC"),
            PCWSTR::null(),
            WS_POPUP,
            pt.x,
            pt.y,
            1,
            1,
            None,
            None,
            None,
            None,
        )
    };
    match probe {
        Ok(h) => {
            let probe = Probe(h);
            window_dpi(probe.0)
        }
        Err(e) => {
            win::record(win::Error::from_win("CreateWindowExW", e));
            system_dpi()
        }
    }
}

/// `GetSystemMetricsForDpi`, for example the vertical scroll bar width at `dpi`.
pub fn metric(index: SYSTEM_METRICS_INDEX, dpi: u32) -> i32 {
    // SAFETY: Plain value query with no pointers.
    unsafe { GetSystemMetricsForDpi(index, dpi) }
}

/// Outer window size for a client size in device pixels, with the frame of `style` at `dpi`.
pub fn outer_for_client(
    client: Size,
    style: WINDOW_STYLE,
    ex_style: WINDOW_EX_STYLE,
    dpi: u32,
) -> win::Result<Size> {
    let mut rect = RECT {
        left: 0,
        top: 0,
        right: client.w,
        bottom: client.h,
    };
    // SAFETY: `rect` is a valid, writable RECT for the duration of the call.
    unsafe { AdjustWindowRectExForDpi(&mut rect, style, false, ex_style, dpi) }
        .map_err(|e| win::Error::from_win("AdjustWindowRectExForDpi", e))?;
    Ok(Size {
        w: rect.right - rect.left,
        h: rect.bottom - rect.top,
    })
}

/// An owned GDI font created for one DPI; deleted on drop.
#[derive(Debug)]
pub struct Font {
    handle: HFONT,
    spec: FontSpec,
    dpi: u32,
}

impl Font {
    /// Creates the GDI font WinForms would create for `spec` on a monitor with `dpi`.
    pub fn new(spec: FontSpec, dpi: u32) -> win::Result<Self> {
        let mut lf = LOGFONTW {
            lfHeight: font_height(spec.points, dpi),
            lfWeight: spec.weight,
            lfCharSet: DEFAULT_CHARSET,
            lfOutPrecision: OUT_DEFAULT_PRECIS,
            lfClipPrecision: CLIP_DEFAULT_PRECIS,
            lfQuality: DEFAULT_QUALITY,
            lfPitchAndFamily: FF_DONTCARE.0,
            ..Default::default()
        };
        let face = to_wide(face_for(spec));
        // Leave room for the terminating NUL that LOGFONTW requires.
        let len = face.len().min(lf.lfFaceName.len()) - 1;
        lf.lfFaceName[..len].copy_from_slice(&face[..len]);
        // SAFETY: `lf` is fully initialized and NUL-terminated.
        let handle = unsafe { CreateFontIndirectW(&lf) };
        if handle.is_invalid() {
            return Err(win::Error::msg("CreateFontIndirectW", spec.face));
        }
        Ok(Self { handle, spec, dpi })
    }

    /// The raw font handle; valid while `self` lives.
    pub fn handle(&self) -> HFONT {
        self.handle
    }

    /// The spec this font was created from.
    pub fn spec(&self) -> FontSpec {
        self.spec
    }

    /// The DPI this font was created for.
    pub fn dpi(&self) -> u32 {
        self.dpi
    }

    /// The face GDI selected for this font (empty when the query fails).
    pub fn selected_face(&self) -> String {
        use windows::Win32::Graphics::Gdi::{
            CreateCompatibleDC, DeleteDC, GetTextFaceW, SelectObject,
        };
        let mut name = [0u16; 64];
        // SAFETY: A memory DC owned by this call; the font is live; the buffer is writable.
        let n = unsafe {
            let dc = CreateCompatibleDC(None);
            let old = SelectObject(dc, self.handle.into());
            let n = GetTextFaceW(dc, Some(&mut name));
            SelectObject(dc, old);
            let _ = DeleteDC(dc);
            n
        };
        String::from_utf16_lossy(&name[..(n.max(1) as usize - 1).min(name.len())])
    }
}

impl Drop for Font {
    fn drop(&mut self) {
        // SAFETY: The handle was created by CreateFontIndirectW and is owned only by `self`;
        // controls that use it are re-fonted or destroyed before the owning cache drops it.
        // A failed delete cannot be reported from Drop; it only leaks one GDI object.
        unsafe {
            let _ = DeleteObject(self.handle.into());
        }
    }
}

/// Makes the calling process PerMonitorV2 aware; the app gets this from its manifest instead.
#[cfg(test)]
pub fn set_per_monitor_v2_for_tests() -> bool {
    use windows::Win32::UI::HiDpi::{
        DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, SetProcessDpiAwarenessContext,
    };
    // SAFETY: Process-wide setting with a predefined context value; called before any window.
    unsafe { SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2) }.is_ok()
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;
    use windows::Win32::Graphics::Gdi::{
        CreateCompatibleDC, DeleteDC, GetTextFaceW, GetTextMetricsW, SelectObject, TEXTMETRICW,
    };

    /// The embedded fonts register, each weight resolves to its own Inter face (not a GDI
    /// substitute), and the type-scale pixel sizes land exactly at 96 DPI.
    #[test]
    fn embedded_inter_resolves_by_weight() {
        assert!(load_fonts(), "AddFontMemResourceEx");
        // SAFETY: A memory DC owned by this test; deleted at the end.
        let dc = unsafe { CreateCompatibleDC(None) };
        for (spec, face, height) in [
            (theme::BODY_FONT, "Inter", 13),
            (theme::BUTTON_FONT, "Inter Medium", 13),
            (theme::TITLE_FONT, "Inter SemiBold", 15),
            (theme::CAPTION_FONT, "Inter SemiBold", 11),
        ] {
            let font = Font::new(spec, BASE_DPI).unwrap();
            let mut name = [0u16; 64];
            let mut tm = TEXTMETRICW::default();
            // SAFETY: The font and DC are live; the buffers are writable.
            let (n, ok) = unsafe {
                let old = SelectObject(dc, font.handle().into());
                let n = GetTextFaceW(dc, Some(&mut name));
                let ok = GetTextMetricsW(dc, &mut tm);
                SelectObject(dc, old);
                (n, ok)
            };
            assert!(ok.as_bool());
            let got = String::from_utf16_lossy(&name[..n.max(1) as usize - 1]);
            assert_eq!(got, face, "{spec:?}");
            assert_eq!(font_height(spec.points, BASE_DPI), -height, "{spec:?}");
            println!("{face}: tmHeight {} for {height} px", tm.tmHeight);
        }
        // SAFETY: Deletes the DC created above.
        unsafe {
            let _ = DeleteDC(dc);
        }
    }
}
