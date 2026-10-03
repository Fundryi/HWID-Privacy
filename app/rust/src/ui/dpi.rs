//! Owned by WP-10a: DPI scaling rules, per-DPI fonts, and window size math (PerMonitorV2).
//!
//! Every rounding rule lives here so the 1.11 baseline can correct it in one place.

use super::layout::Size;
use super::theme::FontSpec;
use crate::win::{self, wide::to_wide};
use windows::Win32::{
    Foundation::{HWND, RECT},
    Graphics::Gdi::{
        CLIP_DEFAULT_PRECIS, CreateFontIndirectW, DEFAULT_CHARSET, DEFAULT_QUALITY, DeleteObject,
        FF_DONTCARE, FW_BOLD, FW_NORMAL, HFONT, LOGFONTW, OUT_DEFAULT_PRECIS,
    },
    UI::{
        HiDpi::{AdjustWindowRectExForDpi, GetDpiForWindow, GetSystemMetricsForDpi},
        WindowsAndMessaging::{SYSTEM_METRICS_INDEX, WINDOW_EX_STYLE, WINDOW_STYLE},
    },
};

/// The DPI at which every theme constant is defined.
pub const BASE_DPI: u32 = 96;

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
            lfWeight: if spec.bold { FW_BOLD.0 } else { FW_NORMAL.0 } as i32,
            lfItalic: u8::from(spec.italic),
            lfCharSet: DEFAULT_CHARSET,
            lfOutPrecision: OUT_DEFAULT_PRECIS,
            lfClipPrecision: CLIP_DEFAULT_PRECIS,
            lfQuality: DEFAULT_QUALITY,
            lfPitchAndFamily: FF_DONTCARE.0,
            ..Default::default()
        };
        let face = to_wide(spec.face);
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
