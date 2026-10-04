//! Native open-file picker; its chrome follows Windows rather than the app's painted kit.

use super::{Error, Result, record, wide::to_wide};
use std::path::{Path, PathBuf};
use windows::Win32::Foundation::{ERROR_CANCELLED, HWND};
use windows::Win32::System::Com::{CLSCTX_INPROC_SERVER, CoCreateInstance, CoTaskMemFree};
use windows::Win32::UI::Shell::Common::COMDLG_FILTERSPEC;
use windows::Win32::UI::Shell::{
    FOS_FILEMUSTEXIST, FOS_FORCEFILESYSTEM, FOS_PATHMUSTEXIST, FileOpenDialog, IFileOpenDialog,
    IShellItem, SHCreateItemFromParsingName, SIGDN_FILESYSPATH,
};
use windows::core::{HRESULT, PCWSTR, PWSTR};

struct TaskString(PWSTR);
impl Drop for TaskString {
    fn drop(&mut self) {
        // SAFETY: IShellItem::GetDisplayName allocates this string with the COM task allocator.
        unsafe {
            CoTaskMemFree(Some(self.0.0.cast()));
        }
    }
}

/// Chooses a plain-text comparison destination, initially beside the executable.
pub(crate) fn save_compare(owner: HWND, initial_dir: &Path) -> Result<Option<PathBuf>> {
    use std::os::windows::ffi::{OsStrExt, OsStringExt};
    use windows::Win32::UI::Shell::{FOS_OVERWRITEPROMPT, FileSaveDialog, IFileSaveDialog};
    let _apartment = super::initialize_com()?;
    // SAFETY: Clock query has no caller-owned pointers and cannot fail.
    let now = unsafe { windows::Win32::System::SystemInformation::GetLocalTime() };
    let filename = to_wide(&format!(
        "HWID-COMPARE-{:02}.{:02}.{:04}-{:02};{:02};{:02}.txt",
        now.wDay, now.wMonth, now.wYear, now.wHour, now.wMinute, now.wSecond
    ));
    let folder: Vec<_> = initial_dir
        .as_os_str()
        .encode_wide()
        .chain(Some(0))
        .collect();
    let title = to_wide("Save comparison as text");
    let name = to_wide("Text files (*.txt)");
    let pattern = to_wide("*.txt");
    let extension = to_wide("txt");
    // SAFETY: COM initialized on this thread; strings outlive the modal dialog. All COM
    // interfaces and the returned task-allocator string are owned by RAII wrappers.
    unsafe {
        let dialog: IFileSaveDialog = CoCreateInstance(&FileSaveDialog, None, CLSCTX_INPROC_SERVER)
            .map_err(|e| Error::from_win("Create save dialog", e))?;
        dialog
            .SetOptions(
                dialog
                    .GetOptions()
                    .map_err(|e| Error::from_win("Read save options", e))?
                    | FOS_FORCEFILESYSTEM
                    | FOS_PATHMUSTEXIST
                    | FOS_OVERWRITEPROMPT,
            )
            .map_err(|e| Error::from_win("Set save options", e))?;
        dialog
            .SetTitle(PCWSTR(title.as_ptr()))
            .map_err(|e| Error::from_win("Set save title", e))?;
        dialog
            .SetFileTypes(&[COMDLG_FILTERSPEC {
                pszName: PCWSTR(name.as_ptr()),
                pszSpec: PCWSTR(pattern.as_ptr()),
            }])
            .map_err(|e| Error::from_win("Set save filter", e))?;
        dialog
            .SetDefaultExtension(PCWSTR(extension.as_ptr()))
            .map_err(|e| Error::from_win("Set save extension", e))?;
        dialog
            .SetFileName(PCWSTR(filename.as_ptr()))
            .map_err(|e| Error::from_win("Set save filename", e))?;
        let item: IShellItem = SHCreateItemFromParsingName(PCWSTR(folder.as_ptr()), None)
            .map_err(|e| Error::from_win("Open save folder", e))?;
        dialog
            .SetFolder(&item)
            .map_err(|e| Error::from_win("Set save folder", e))?;
        match dialog.Show(Some(owner)) {
            Ok(()) => {}
            Err(e) if e.code() == HRESULT::from_win32(ERROR_CANCELLED.0) => return Ok(None),
            Err(e) => return Err(Error::from_win("Show save dialog", e)),
        }
        let item = dialog
            .GetResult()
            .map_err(|e| Error::from_win("Read saved file", e))?;
        let name = TaskString(
            item.GetDisplayName(SIGDN_FILESYSPATH)
                .map_err(|e| Error::from_win("Read save path", e))?,
        );
        Ok(Some(PathBuf::from(std::ffi::OsString::from_wide(
            name.0.as_wide(),
        ))))
    }
}

/// Selects one existing file; filters are (display name, glob pattern) pairs. Cancel is None.
/// Other failures are recorded and also return None, as required by the picker contract.
pub fn open_file(
    owner: HWND,
    title: &str,
    filters: &[(&str, &str)],
    initial_dir: &Path,
) -> Option<PathBuf> {
    match pick(owner, title, filters, initial_dir) {
        Ok(path) => path,
        Err(error) => {
            record(error);
            None
        }
    }
}

fn pick(
    owner: HWND,
    title: &str,
    filters: &[(&str, &str)],
    initial_dir: &Path,
) -> Result<Option<PathBuf>> {
    let _apartment = super::initialize_com()?;
    let title = to_wide(title);
    use std::os::windows::ffi::OsStrExt;
    let folder: Vec<u16> = initial_dir
        .as_os_str()
        .encode_wide()
        .chain(Some(0))
        .collect();
    let names: Vec<_> = filters
        .iter()
        .map(|(n, p)| (to_wide(n), to_wide(p)))
        .collect();
    let specs: Vec<_> = names
        .iter()
        .map(|(n, p)| COMDLG_FILTERSPEC {
            pszName: PCWSTR(n.as_ptr()),
            pszSpec: PCWSTR(p.as_ptr()),
        })
        .collect();
    // SAFETY: COM is initialized on this thread; all strings and filter arrays live through
    // Show, the dialog and shell item are RAII COM interfaces, and owner is borrowed only.
    unsafe {
        let dialog: IFileOpenDialog = CoCreateInstance(&FileOpenDialog, None, CLSCTX_INPROC_SERVER)
            .map_err(|e| Error::from_win("Create file dialog", e))?;
        dialog
            .SetTitle(PCWSTR(title.as_ptr()))
            .map_err(|e| Error::from_win("Set file dialog title", e))?;
        let options = dialog
            .GetOptions()
            .map_err(|e| Error::from_win("Read file dialog options", e))?;
        dialog
            .SetOptions(options | FOS_FORCEFILESYSTEM | FOS_FILEMUSTEXIST | FOS_PATHMUSTEXIST)
            .map_err(|e| Error::from_win("Set file dialog options", e))?;
        if !specs.is_empty() {
            // The crate binding expects the slice length to fit a Win32 UINT.
            u32::try_from(specs.len())
                .map_err(|_| Error::msg("Set file filters", "too many filters"))?;
            dialog
                .SetFileTypes(&specs)
                .map_err(|e| Error::from_win("Set file filters", e))?;
        }
        let item: IShellItem = SHCreateItemFromParsingName(PCWSTR(folder.as_ptr()), None)
            .map_err(|e| Error::from_win("Open initial folder", e))?;
        dialog
            .SetFolder(&item)
            .map_err(|e| Error::from_win("Set initial folder", e))?;
        match dialog.Show(Some(owner)) {
            Ok(()) => {}
            Err(error) if error.code() == HRESULT::from_win32(ERROR_CANCELLED.0) => {
                return Ok(None);
            }
            Err(error) => return Err(Error::from_win("Show file dialog", error)),
        }
        let item = dialog
            .GetResult()
            .map_err(|e| Error::from_win("Read selected file", e))?;
        let name = TaskString(
            item.GetDisplayName(SIGDN_FILESYSPATH)
                .map_err(|e| Error::from_win("Read selected path", e))?,
        );
        use std::os::windows::ffi::OsStringExt;
        Ok(Some(PathBuf::from(std::ffi::OsString::from_wide(
            name.0.as_wide(),
        ))))
    }
}
