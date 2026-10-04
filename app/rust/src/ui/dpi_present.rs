//! A temporary client snapshot while native child surfaces change DPI.

use super::layout::Size;
use crate::win;
use windows::{
    Win32::{
        Foundation::{COLORREF, HWND, POINT, SIZE},
        Graphics::{
            Dwm::DwmFlush,
            Gdi::{
                BitBlt, CreateCompatibleBitmap, CreateCompatibleDC, DCX_CACHE, DeleteDC,
                DeleteObject, GetDCEx, HALFTONE, HBITMAP, HDC, HGDIOBJ, ReleaseDC, SRCCOPY,
                SelectObject, SetBrushOrgEx, SetStretchBltMode, StretchBlt,
            },
        },
        UI::WindowsAndMessaging::{
            CreateWindowExW, DestroyWindow, HWND_TOP, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE,
            SWP_SHOWWINDOW, SetWindowPos, ULW_OPAQUE, UpdateLayeredWindow, WS_CHILD, WS_EX_LAYERED,
            WS_EX_NOACTIVATE, WS_EX_TRANSPARENT,
        },
    },
    core::w,
};

struct Surface {
    dc: HDC,
    bitmap: HBITMAP,
    previous: HGDIOBJ,
    size: Size,
}

impl Surface {
    fn new(reference: HDC, size: Size) -> Option<Self> {
        // SAFETY: These compatible GDI resources are owned by the returned guard.
        unsafe {
            let dc = CreateCompatibleDC(Some(reference));
            if dc.is_invalid() {
                win::record(win::Error::last("CreateCompatibleDC"));
                return None;
            }
            let bitmap = CreateCompatibleBitmap(reference, size.w, size.h);
            if bitmap.is_invalid() {
                win::record(win::Error::last("CreateCompatibleBitmap"));
                let _ = DeleteDC(dc);
                return None;
            }
            let previous = SelectObject(dc, bitmap.into());
            if previous.is_invalid() {
                win::record(win::Error::last("SelectObject"));
                let _ = DeleteObject(bitmap.into());
                let _ = DeleteDC(dc);
                return None;
            }
            Some(Self {
                dc,
                bitmap,
                previous,
                size,
            })
        }
    }
}

impl Drop for Surface {
    fn drop(&mut self) {
        // SAFETY: Restore the DC's original bitmap before deleting the owned bitmap and DC.
        unsafe {
            SelectObject(self.dc, self.previous);
            let _ = DeleteObject(self.bitmap.into());
            let _ = DeleteDC(self.dc);
        }
    }
}

pub(super) struct DpiCover {
    hwnd: HWND,
    image: Surface,
}

impl DpiCover {
    /// Preserves the visible client while native descendants are re-fonted and laid out.
    pub(super) fn new(parent: HWND, size: Size) -> Option<Self> {
        if size.w <= 0 || size.h <= 0 {
            return None;
        }
        // SAFETY: Hidden/minimized forms have no presented client surface to preserve.
        unsafe {
            if !windows::Win32::UI::WindowsAndMessaging::IsWindowVisible(parent).as_bool()
                || windows::Win32::UI::WindowsAndMessaging::IsIconic(parent).as_bool()
            {
                return None;
            }
        }
        // SAFETY: A cached client DC without child clipping reads our last complete surface.
        let dc = unsafe { GetDCEx(Some(parent), None, DCX_CACHE) };
        if dc.is_invalid() {
            win::record(win::Error::last("GetDCEx"));
            return None;
        }
        let image = Surface::new(dc, size);
        let copied = image.as_ref().is_some_and(|image| {
            // SAFETY: Both DCs cover the supplied client rectangle.
            match unsafe { BitBlt(image.dc, 0, 0, size.w, size.h, Some(dc), 0, 0, SRCCOPY) } {
                Ok(()) => true,
                Err(e) => {
                    win::record(win::Error::from_win("BitBlt", e));
                    false
                }
            }
        });
        // SAFETY: Balances GetDCEx above, including allocation failure.
        unsafe {
            ReleaseDC(Some(parent), dc);
        }
        if !copied {
            return None;
        }
        let image = image?;
        // SAFETY: A non-activating, hit-test-transparent child, owned by this guard. No callback
        // or application state is attached; the native STATIC class needs no registration.
        let hwnd = unsafe {
            CreateWindowExW(
                WS_EX_LAYERED | WS_EX_TRANSPARENT | WS_EX_NOACTIVATE,
                w!("STATIC"),
                w!(""),
                WS_CHILD,
                0,
                0,
                size.w,
                size.h,
                Some(parent),
                None,
                None,
                None,
            )
        }
        .map_err(|e| win::record(win::Error::from_win("CreateWindowExW (DPI cover)", e)))
        .ok()?;
        let cover = Self { hwnd, image };
        if !cover.present(cover.image.dc, size) {
            return None;
        }
        // SAFETY: Show only this child above its siblings without moving keyboard focus.
        unsafe {
            if let Err(e) = SetWindowPos(
                hwnd,
                Some(HWND_TOP),
                0,
                0,
                0,
                0,
                SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE | SWP_SHOWWINDOW,
            ) {
                win::record(win::Error::from_win("SetWindowPos (DPI cover)", e));
                return None;
            }
        }
        Some(cover)
    }

    /// Stretches one coherent previous frame to cover the new client, never child by child.
    pub(super) fn resize(&self, size: Size) {
        if size.w <= 0 || size.h <= 0 || size == self.image.size {
            return;
        }
        let Some(scaled) = Surface::new(self.image.dc, size) else {
            return;
        };
        // SAFETY: Scale the complete prior image, then atomically replace the layer's bitmap.
        unsafe {
            SetStretchBltMode(scaled.dc, HALFTONE);
            let _ = SetBrushOrgEx(scaled.dc, 0, 0, None);
            if !StretchBlt(
                scaled.dc,
                0,
                0,
                size.w,
                size.h,
                Some(self.image.dc),
                0,
                0,
                self.image.size.w,
                self.image.size.h,
                SRCCOPY,
            )
            .as_bool()
            {
                win::record(win::Error::last("StretchBlt"));
                return;
            }
        }
        self.present(scaled.dc, size);
    }

    fn present(&self, dc: HDC, size: Size) -> bool {
        // SAFETY: The source DC contains the whole opaque client image; Windows copies it.
        unsafe {
            if let Err(e) = UpdateLayeredWindow(
                self.hwnd,
                None,
                None,
                Some(&SIZE {
                    cx: size.w,
                    cy: size.h,
                }),
                Some(dc),
                Some(&POINT::default()),
                COLORREF(0),
                None,
                ULW_OPAQUE,
            ) {
                win::record(win::Error::from_win("UpdateLayeredWindow", e));
                return false;
            }
        }
        true
    }
}

impl Drop for DpiCover {
    fn drop(&mut self) {
        // SAFETY: Flush the completed native repaint before revealing it; destroy only our
        // temporary child. Neither call changes the form's focus or owns any native EDIT data.
        unsafe {
            if let Err(e) = DwmFlush() {
                win::record(win::Error::from_win("DwmFlush", e));
            }
            if let Err(e) = DestroyWindow(self.hwnd) {
                win::record(win::Error::from_win("DestroyWindow (DPI cover)", e));
            }
        }
    }
}
