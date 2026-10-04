//! Owned Unicode clipboard transfer for compare row and whole-table copying.

use super::*;

pub(crate) fn copy_text(owner: HWND, text: &str) {
    if let Err(error) = clipboard(owner, text) {
        win::record(error);
    }
}

fn clipboard(owner: HWND, text: &str) -> win::Result<()> {
    use windows::Win32::Foundation::{GlobalFree, HGLOBAL};
    use windows::Win32::System::DataExchange::{
        CloseClipboard, EmptyClipboard, OpenClipboard, SetClipboardData,
    };
    use windows::Win32::System::Memory::{GMEM_MOVEABLE, GlobalAlloc, GlobalLock, GlobalUnlock};
    struct Clipboard;
    impl Drop for Clipboard {
        fn drop(&mut self) {
            // SAFETY: This guard follows a successful OpenClipboard on this thread.
            if let Err(error) = unsafe { CloseClipboard() } {
                win::record(win::Error::from_win("CloseClipboard", error));
            }
        }
    }
    struct Memory(HGLOBAL);
    impl Drop for Memory {
        fn drop(&mut self) {
            if !self.0.is_invalid() {
                // SAFETY: Owned allocation, unlocked before this guard is dropped.
                if let Err(error) = unsafe { GlobalFree(Some(self.0)) } {
                    win::record(win::Error::from_win("GlobalFree", error));
                }
            }
        }
    }
    let wide = to_wide(text);
    // SAFETY: The live owner and global movable memory follow the clipboard ownership contract.
    unsafe {
        OpenClipboard(Some(owner)).map_err(|e| win::Error::from_win("OpenClipboard", e))?;
        let _clipboard = Clipboard;
        let mut memory = Memory(
            GlobalAlloc(GMEM_MOVEABLE, wide.len() * 2)
                .map_err(|e| win::Error::from_win("GlobalAlloc", e))?,
        );
        let dest = GlobalLock(memory.0);
        if dest.is_null() {
            return Err(win::Error::msg(
                "GlobalLock",
                "Could not lock clipboard text",
            ));
        }
        std::ptr::copy_nonoverlapping(wide.as_ptr(), dest.cast(), wide.len());
        // A zero return after the final unlock means success, not a failed copy.
        let _ = GlobalUnlock(memory.0);
        EmptyClipboard().map_err(|e| win::Error::from_win("EmptyClipboard", e))?;
        SetClipboardData(13, Some(HANDLE(memory.0.0)))
            .map_err(|e| win::Error::from_win("SetClipboardData", e))?;
        memory.0 = HGLOBAL::default();
    }
    Ok(())
}
