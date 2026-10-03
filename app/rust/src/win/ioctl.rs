//! Owned device handles and bounded IOCTL buffers.

use super::{Error, OwnedHandle, Result};

/// Opens an absolute device path with the requested Win32 access mask.
pub fn open_device(_path: &str, _access: u32) -> Result<OwnedHandle> {
    Err(Error::msg("CreateFileW device", "not ported yet"))
}
/// Executes an IOCTL with bounded, grow-and-retry output allocation.
pub fn device_io_control(
    _h: &OwnedHandle,
    _code: u32,
    _input: &[u8],
    _out_capacity: usize,
) -> Result<Vec<u8>> {
    Err(Error::msg("DeviceIoControl", "not ported yet"))
}
