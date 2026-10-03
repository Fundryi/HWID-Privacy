//! Owned device handles and bounded IOCTL buffers.

use super::{Error, OwnedHandle, Result, wide};
use windows::Win32::Foundation::{ERROR_INSUFFICIENT_BUFFER, ERROR_MORE_DATA};
use windows::Win32::Storage::FileSystem::{
    CreateFileW, FILE_FLAGS_AND_ATTRIBUTES, FILE_SHARE_DELETE, FILE_SHARE_READ, FILE_SHARE_WRITE,
    OPEN_EXISTING,
};
use windows::Win32::System::IO::DeviceIoControl;
use windows::core::PCWSTR;

const MAX_OUTPUT: usize = 16 * 1024 * 1024;
const MAX_ATTEMPTS: usize = 4;

/// Opens an absolute device path with the requested Win32 access mask.
pub fn open_device(path: &str, access: u32) -> Result<OwnedHandle> {
    if !(path.starts_with(r"\\.\") || path.starts_with(r"\\?\")) || path.contains('\0') {
        return Err(Error::msg(
            "CreateFileW device",
            "expected an absolute Windows device path",
        ));
    }
    let path = wide::to_wide(path);
    // C# parity: Services/Win32/StorageDeviceIdQuery.cs:204
    // SAFETY: The terminated device path is live; no security/template pointers are supplied.
    let handle = unsafe {
        CreateFileW(
            PCWSTR(path.as_ptr()),
            access,
            FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
            None,
            OPEN_EXISTING,
            FILE_FLAGS_AND_ATTRIBUTES(0),
            None,
        )
    }
    .map_err(|e| Error::from_win("CreateFileW device", e))?;
    // SAFETY: CreateFileW returned a uniquely owned, non-overlapped kernel handle.
    unsafe { OwnedHandle::from_raw(handle) }
}
/// Executes an IOCTL with bounded, grow-and-retry output allocation.
pub fn device_io_control(
    h: &OwnedHandle,
    code: u32,
    input: &[u8],
    out_capacity: usize,
) -> Result<Vec<u8>> {
    let input_length = u32::try_from(input.len())
        .map_err(|_| Error::msg("DeviceIoControl", "input exceeds the Win32 buffer size"))?;
    retry_output(out_capacity, |output| {
        let mut returned = 0;
        // SAFETY: h stays borrowed; both buffers are live for their stated lengths.
        // This is a synchronous call with a writable bytes-returned pointer, no OVERLAPPED.
        unsafe {
            DeviceIoControl(
                h.as_raw(),
                code,
                (!input.is_empty()).then_some(input.as_ptr().cast()),
                input_length,
                Some(output.as_mut_ptr().cast()),
                output.len() as u32,
                Some(&mut returned),
                None,
            )
        }
        .map_err(|e| {
            let mut error = Error::from_win("DeviceIoControl", e);
            // The wrapper returns HRESULT_FROM_WIN32; retry decisions need the original code.
            if error.code & 0xFFFF0000 == 0x80070000 {
                error.code &= 0xFFFF;
            }
            error
        })?;
        Ok(returned as usize)
    })
}

fn retry_output(
    mut capacity: usize,
    mut call: impl FnMut(&mut [u8]) -> Result<usize>,
) -> Result<Vec<u8>> {
    for attempt in 0..MAX_ATTEMPTS {
        if capacity == 0 || capacity > MAX_OUTPUT {
            return Err(Error::msg(
                "DeviceIoControl",
                "output capacity must be between 1 byte and 16 MiB",
            ));
        }
        let mut output = Vec::new();
        output
            .try_reserve_exact(capacity)
            .map_err(|e| Error::msg("DeviceIoControl", format!("output allocation: {e}")))?;
        output.resize(capacity, 0);
        // C# parity: Services/Win32/StorageDeviceIdQuery.cs:230
        let returned = match call(&mut output) {
            Err(error)
                if (error.code == ERROR_INSUFFICIENT_BUFFER.0
                    || error.code == ERROR_MORE_DATA.0)
                    && attempt < MAX_ATTEMPTS - 1 =>
            {
                capacity *= 2;
                continue;
            }
            Err(error) => return Err(error),
            Ok(0) => return Err(Error::msg("DeviceIoControl", "No data returned")),
            Ok(returned) => returned,
        };
        // C# parity: Services/Win32/StorageDeviceIdQuery.cs:263
        if returned >= capacity && attempt < MAX_ATTEMPTS - 1 {
            capacity *= 2;
            continue;
        }
        // C# parity: Services/Win32/StorageDeviceIdQuery.cs:269
        output.truncate(returned.min(capacity));
        return Ok(output);
    }
    Err(Error::msg("DeviceIoControl", "Buffer too small"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use windows::Win32::Foundation::{ERROR_ACCESS_DENIED, ERROR_INVALID_FUNCTION, GENERIC_READ};

    #[test]
    fn physical_drive_read_without_admin_has_a_readable_error() {
        assert!(
            !super::super::security::is_admin(),
            "run this check without elevation"
        );
        let error = open_device(r"\\.\PhysicalDrive0", GENERIC_READ.0)
            .expect_err("this check must return an error");
        assert_eq!(error.code & 0xFFFF, ERROR_ACCESS_DENIED.0);
        assert!(error.to_string().contains("CreateFileW device failed:"));
        assert!(!error.detail.is_empty());
    }

    #[test]
    #[ignore = "requires administrator access to PhysicalDrive0"]
    fn physical_drive_ioctl_requires_admin() {
        use windows::Win32::System::Ioctl::{
            IOCTL_STORAGE_GET_DEVICE_NUMBER, STORAGE_DEVICE_NUMBER,
        };
        let handle = open_device(r"\\.\PhysicalDrive0", GENERIC_READ.0)
            .expect("physical drive must open in an elevated shell");
        let bytes = device_io_control(&handle, IOCTL_STORAGE_GET_DEVICE_NUMBER, &[], 4)
            .expect("physical drive must return a device number after buffer growth");
        assert_eq!(bytes.len(), std::mem::size_of::<STORAGE_DEVICE_NUMBER>());
    }

    #[test]
    fn retries_buffer_errors_and_full_replies_then_trims() {
        let mut capacities = Vec::new();
        let result = retry_output(2, |output| {
            capacities.push(output.len());
            match capacities.len() {
                1 | 2 => Err(Error {
                    op: "DeviceIoControl",
                    code: if capacities.len() == 1 {
                        ERROR_INSUFFICIENT_BUFFER.0
                    } else {
                        ERROR_MORE_DATA.0
                    },
                    detail: "fabricated short buffer".to_owned(),
                }),
                3 => Ok(output.len()),
                _ => {
                    output[..3].copy_from_slice(&[1, 2, 3]);
                    Ok(3)
                }
            }
        })
        .expect("Windows check should succeed");
        assert_eq!(capacities, [2, 4, 8, 16]);
        assert_eq!(result, [1, 2, 3]);
    }

    #[test]
    fn bounds_attempts_and_rejects_empty_or_oversized_buffers() {
        let mut calls = 0;
        let error = retry_output(1, |_| {
            calls += 1;
            Err(Error {
                op: "DeviceIoControl",
                code: ERROR_MORE_DATA.0,
                detail: "fabricated".to_owned(),
            })
        })
        .expect_err("this check must return an error");
        assert_eq!(calls, 4);
        assert_eq!(error.code, ERROR_MORE_DATA.0);
        assert!(retry_output(1, |_| Ok(0)).is_err());
        assert!(retry_output(0, |_| panic!("must not call")).is_err());
        assert!(retry_output(MAX_OUTPUT + 1, |_| panic!("must not call")).is_err());
        assert!(open_device("PhysicalDrive0", 0).is_err());
        assert!(open_device("\\\\.\\PhysicalDrive0\0ignored", 0).is_err());
        let handle = open_device(r"\\.\NUL", 0).expect("NUL is accessible without elevation");
        let error =
            device_io_control(&handle, 0, &[], 16).expect_err("NUL does not implement this IOCTL");
        assert_eq!(error.code, ERROR_INVALID_FUNCTION.0);
        assert!(!error.detail.is_empty());
    }
}
