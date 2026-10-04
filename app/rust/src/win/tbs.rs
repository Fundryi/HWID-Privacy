//! Bounded read-only TBS version query; no TPM context, command or key use.

use super::{Error, Result, dll};
use std::{ffi::c_void, sync::mpsc, time::Duration};
use windows::Win32::System::TpmBaseServices::{
    TBS_SUCCESS, TPM_DEVICE_INFO, TPM_VERSION_12, TPM_VERSION_20,
};

/// Reads TPM version independently of WMI status, with a two-second caller bound.
pub fn device_version() -> Result<Option<(u8, u8)>> {
    let (sender, receiver) = mpsc::channel();
    std::thread::Builder::new()
        .name("tbs-device-info".into())
        .spawn(move || {
            let result = super::catch_panic(read_device_version).unwrap_or_else(|_| {
                Err(Error::msg(
                    "Tbsi_GetDeviceInfo",
                    "malformed: query worker panicked",
                ))
            });
            let _ = sender.send(result);
        })
        .map_err(|_| Error::msg("Tbsi_GetDeviceInfo", "unable to start query worker"))?;
    receiver
        .recv_timeout(Duration::from_secs(2))
        .map_err(|error| match error {
            mpsc::RecvTimeoutError::Timeout => Error {
                op: "Tbsi_GetDeviceInfo",
                code: 1460,
                detail: "query exceeded two seconds".into(),
            },
            mpsc::RecvTimeoutError::Disconnected => {
                Error::msg("Tbsi_GetDeviceInfo", "query worker ended without a result")
            }
        })?
}

fn read_device_version() -> Result<Option<(u8, u8)>> {
    // Runtime loading preserves the existing import allow-list and missing-DLL path.
    let library = dll::load_system_dll("tbs.dll")?;
    let address = library.proc_address(c"Tbsi_GetDeviceInfo").ok_or(Error {
        op: "Tbsi_GetDeviceInfo",
        code: 127,
        detail: "API is unavailable".into(),
    })?;
    type GetDeviceInfo = unsafe extern "system" fn(u32, *mut c_void) -> u32;
    // SAFETY: tbs.h defines this exact exported signature; library stays loaded
    // in this worker until the call returns, even if its receiver times out.
    let get_device_info: GetDeviceInfo = unsafe { std::mem::transmute(address) };
    let mut info = TPM_DEVICE_INFO {
        structVersion: TPM_VERSION_20,
        ..Default::default()
    };
    // SAFETY: info is aligned writable SDK storage of exactly the advertised size.
    // Tbsi_GetDeviceInfo reads device metadata without a context or TPM command.
    let status = unsafe {
        get_device_info(
            std::mem::size_of::<TPM_DEVICE_INFO>() as u32,
            (&mut info as *mut TPM_DEVICE_INFO).cast(),
        )
    };
    decode_device_version(status, info.tpmVersion)
    // The SDK reserves interface type and implementation revision.
}

fn decode_device_version(status: u32, version: u32) -> Result<Option<(u8, u8)>> {
    match status {
        TBS_SUCCESS => match version {
            TPM_VERSION_12 => Ok(Some((1, 2))),
            TPM_VERSION_20 => Ok(Some((2, 0))),
            _ => Err(Error::msg(
                "Tbsi_GetDeviceInfo",
                "unsupported: TPM version result",
            )),
        },
        0x8028_400F => Ok(None), // TBS_E_TPM_NOT_FOUND; distinct from query failure.
        code => Err(Error {
            op: "Tbsi_GetDeviceInfo",
            code,
            detail: match code {
                5 | 0x8028_4012 => "access-denied: device information query",
                127 => "unsupported: API absent",
                126 => "absent: system DLL",
                1460 => "timeout: device information query",
                _ => "unavailable: device information query failed",
            }
            .into(),
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn device_info_failures_and_tpm12_are_independent_of_firmware() {
        assert_eq!(
            decode_device_version(0, TPM_VERSION_12).unwrap(),
            Some((1, 2))
        );
        assert_eq!(
            decode_device_version(0, TPM_VERSION_20).unwrap(),
            Some((2, 0))
        );
        assert_eq!(decode_device_version(0x8028_400F, u32::MAX).unwrap(), None);
        for (status, class) in [
            (5, "access-denied"),
            (0x8028_4012, "access-denied"),
            (127, "unsupported"),
            (126, "absent"),
            (1460, "timeout"),
            (0x8028_4002, "unavailable"),
        ] {
            let error = decode_device_version(status, 0).unwrap_err();
            assert_eq!(error.code, status);
            assert!(error.detail.contains(class));
        }
        assert!(decode_device_version(0, u32::MAX).is_err());
    }
}
