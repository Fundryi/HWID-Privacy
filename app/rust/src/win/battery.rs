//! Read-only battery interfaces and independently queried identity fields.

use super::{Error, OwnedHandle, Result, ioctl};
use std::{
    sync::{Arc, mpsc},
    time::Duration,
};
use windows::{
    Win32::{
        Devices::DeviceAndDriverInstallation::{
            CM_GET_DEVICE_INTERFACE_LIST_PRESENT, CM_Get_Device_Interface_List_SizeW,
            CM_Get_Device_Interface_ListW, CM_MapCrToWin32Err, CR_BUFFER_SMALL, CR_SUCCESS,
        },
        Foundation::{ERROR_FILE_NOT_FOUND, ERROR_GEN_FAILURE, ERROR_NO_SUCH_DEVICE, GENERIC_READ},
        System::{
            IO::DeviceIoControl,
            Power::{
                BatteryDeviceName, BatteryManufactureDate, BatteryManufactureName,
                BatterySerialNumber, BatteryUniqueID, GUID_DEVICE_BATTERY,
                IOCTL_BATTERY_QUERY_INFORMATION, IOCTL_BATTERY_QUERY_TAG,
            },
        },
    },
    core::PCWSTR,
};

const WAIT: Duration = Duration::from_millis(500);
const MAX_STRING_BYTES: usize = 65536;

pub struct Field {
    pub label: &'static str,
    pub identity: bool,
    pub value: Result<Option<String>>,
}

pub struct Battery {
    pub fields: Vec<Field>,
}

#[derive(Default)]
pub struct Scan {
    pub batteries: Vec<Battery>,
    pub failures: Vec<Error>,
}

/// Enumerates present battery interfaces, not the battery setup class GUID.
pub fn collect() -> Result<Scan> {
    let mut scan = Scan::default();
    for path in interfaces()? {
        match path.and_then(|path| read_battery(&path, &mut scan.failures)) {
            Ok(Some(battery)) => scan.batteries.push(battery),
            Ok(None) => {}
            Err(error) => scan.batteries.push(Battery {
                fields: vec![Field {
                    label: "Battery Information",
                    identity: false,
                    value: Err(error),
                }],
            }),
        }
    }
    Ok(scan)
}

fn interfaces() -> Result<Vec<Result<String>>> {
    for _ in 0..3 {
        let mut count = 0;
        // SAFETY: GUID and count are live; the null ID requests all present interfaces.
        let status = unsafe {
            CM_Get_Device_Interface_List_SizeW(
                &mut count,
                &GUID_DEVICE_BATTERY,
                PCWSTR::null(),
                CM_GET_DEVICE_INTERFACE_LIST_PRESENT,
            )
        };
        if status != CR_SUCCESS {
            return Err(config_error(status));
        }
        if !(1..=65536).contains(&count) {
            return Err(Error::msg(
                "battery interfaces",
                "malformed interface list size",
            ));
        }
        let mut buffer = vec![0; count as usize];
        // SAFETY: The writable UTF-16 buffer has the queried capacity.
        let status = unsafe {
            CM_Get_Device_Interface_ListW(
                &GUID_DEVICE_BATTERY,
                PCWSTR::null(),
                &mut buffer,
                CM_GET_DEVICE_INTERFACE_LIST_PRESENT,
            )
        };
        if status == CR_BUFFER_SMALL {
            continue;
        }
        if status != CR_SUCCESS {
            return Err(config_error(status));
        }
        if buffer.last() != Some(&0) {
            return Err(Error::msg(
                "battery interfaces",
                "unterminated interface list",
            ));
        }
        let mut paths = buffer
            .split(|unit| *unit == 0)
            .take_while(|path| !path.is_empty())
            .map(|path| {
                String::from_utf16(path)
                    .map_err(|_| Error::msg("battery interfaces", "malformed UTF-16 path"))
            })
            .collect::<Vec<_>>();
        if paths.len() > 64 {
            return Err(Error::msg(
                "battery interfaces",
                "interface count exceeds 64",
            ));
        }
        paths.sort_by(|a, b| a.as_ref().ok().cmp(&b.as_ref().ok()));
        paths.dedup_by(|a, b| a.as_ref().ok().is_some() && a.as_ref().ok() == b.as_ref().ok());
        return Ok(paths);
    }
    Err(Error::msg(
        "battery interfaces",
        "interface list kept growing",
    ))
}

fn config_error(status: windows::Win32::Devices::DeviceAndDriverInstallation::CONFIGRET) -> Error {
    // SAFETY: This pure mapping takes a status code and has no pointer arguments.
    let code = unsafe { CM_MapCrToWin32Err(status, ERROR_GEN_FAILURE.0) };
    Error {
        op: "battery interfaces",
        code,
        detail: "interface enumeration unavailable".to_owned(),
    }
}

fn read_battery(path: &str, failures: &mut Vec<Error>) -> Result<Option<Battery>> {
    let handle = Arc::new(ioctl::open_device(path, GENERIC_READ.0)?);
    for attempt in 0..2 {
        let Some(tag) = query_tag(&handle)? else {
            return Ok(None);
        };
        let mut fields = Vec::new();
        let mut stale = None;
        for (label, identity, level) in [
            ("Battery Name", false, BatteryDeviceName),
            ("Battery Manufacturer", false, BatteryManufactureName),
            ("Battery Manufacture Date", false, BatteryManufactureDate),
            ("Battery Serial", true, BatterySerialNumber),
            ("Battery Unique ID", true, BatteryUniqueID),
        ] {
            // BATTERY_QUERY_INFORMATION is three four-byte fields, AtRate is unused here.
            let mut input = tag.to_le_bytes().to_vec();
            input.extend_from_slice(&level.0.to_le_bytes());
            input.extend_from_slice(&0_u32.to_le_bytes());
            let value = query(
                &handle,
                IOCTL_BATTERY_QUERY_INFORMATION,
                input,
                if level == BatteryManufactureDate {
                    4
                } else {
                    MAX_STRING_BYTES
                },
            )
            .and_then(|bytes| {
                if level == BatteryManufactureDate {
                    parse_date(&bytes)
                } else {
                    parse_string(&bytes)
                }
            });
            if let Err(error) = &value
                && stale_tag(error)
            {
                stale = Some(error.clone());
                break;
            }
            fields.push(Field {
                label,
                identity,
                value,
            });
        }
        if stale.is_none() {
            // Recheck after the last field: battery changes cannot silently mix two tags.
            match query_tag(&handle) {
                Ok(Some(current)) if current == tag => return Ok(Some(Battery { fields })),
                Ok(_) => stale = Some(Error::msg("battery tag", "battery changed during queries")),
                Err(error) if stale_tag(&error) => stale = Some(error),
                Err(error) => {
                    // Unknown final tag does not erase independently successful queries.
                    fields.push(Field {
                        label: "Battery Tag Verification",
                        identity: false,
                        value: Err(error),
                    });
                    return Ok(Some(Battery { fields }));
                }
            }
        }
        if let Some(error) = stale {
            failures.push(error);
        }
        if attempt == 1 {
            return Err(Error::msg(
                "battery tag",
                "battery changed during both query attempts",
            ));
        }
    }
    unreachable!()
}

fn stale_tag(error: &Error) -> bool {
    // Microsoft documents FILE_NOT_FOUND on Windows 10 1809 and earlier.
    error.code == ERROR_NO_SUCH_DEVICE.0 || error.code == ERROR_FILE_NOT_FOUND.0
}

fn query_tag(handle: &Arc<OwnedHandle>) -> Result<Option<u32>> {
    match query(
        handle,
        IOCTL_BATTERY_QUERY_TAG,
        0_u32.to_le_bytes().to_vec(),
        4,
    ) {
        Err(error) if stale_tag(&error) => Ok(None),
        Err(error) => Err(error),
        Ok(bytes) => {
            let tag = u32::from_le_bytes(
                bytes
                    .try_into()
                    .map_err(|_| Error::msg("battery tag", "malformed tag length"))?,
            );
            Ok((tag != 0).then_some(tag))
        }
    }
}

fn query(handle: &Arc<OwnedHandle>, code: u32, input: Vec<u8>, capacity: usize) -> Result<Vec<u8>> {
    let handle = Arc::clone(handle);
    let (send, receive) = mpsc::sync_channel(1);
    // As in storage::bounded, the worker owns all kernel-visible memory until return.
    // Abandoning the caller's 500-ms wait never frees an in-flight request's buffers.
    std::thread::Builder::new()
        .name("battery query".to_owned())
        .spawn(move || {
            let mut output = vec![0_u8; capacity];
            let mut returned = 0;
            // SAFETY: The shared handle, input, output and count remain live through this
            // synchronous read-only IOCTL; no OVERLAPPED or driver-write request is used.
            let result = unsafe {
                DeviceIoControl(
                    handle.as_raw(),
                    code,
                    Some(input.as_ptr().cast()),
                    input.len() as u32,
                    Some(output.as_mut_ptr().cast()),
                    capacity as u32,
                    Some(&mut returned),
                    None,
                )
            }
            .map_err(|e| Error::from_win("battery query", e))
            .and_then(|()| {
                if returned as usize > output.len() {
                    return Err(Error::msg("battery query", "malformed returned length"));
                }
                output.truncate(returned as usize);
                Ok(output)
            });
            let _ = send.send(result);
        })
        .map_err(|_| Error::msg("battery query", "failed to start query worker"))?;
    receive.recv_timeout(WAIT).map_err(|error| {
        Error::msg(
            "battery query",
            match error {
                mpsc::RecvTimeoutError::Timeout => "timed out after 500 ms",
                mpsc::RecvTimeoutError::Disconnected => "query worker disconnected",
            },
        )
    })?
}

fn parse_string(bytes: &[u8]) -> Result<Option<String>> {
    if bytes.is_empty() || bytes.len() > MAX_STRING_BYTES || !bytes.len().is_multiple_of(2) {
        return Err(Error::msg("battery string", "malformed UTF-16 byte count"));
    }
    let units: Vec<_> = bytes
        .as_chunks::<2>()
        .0
        .iter()
        .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
        .collect();
    let end = units
        .iter()
        .position(|unit| *unit == 0)
        .ok_or_else(|| Error::msg("battery string", "unterminated UTF-16 string"))?;
    let value = String::from_utf16(&units[..end])
        .map_err(|_| Error::msg("battery string", "malformed UTF-16 string"))?;
    if value.chars().any(char::is_control) {
        return Err(Error::msg("battery string", "control character in string"));
    }
    Ok((!value.is_empty()).then_some(value))
}

fn parse_date(bytes: &[u8]) -> Result<Option<String>> {
    let [day, month, lo, hi] = *bytes else {
        return Err(Error::msg("battery date", "malformed date length"));
    };
    let year = u16::from_le_bytes([lo, hi]);
    if (year, month, day) == (0, 0, 0) {
        return Ok(None);
    }
    super::firmware::battery_date(year, month, day).map(Some)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn battery_reply_encodings_and_stale_codes() {
        let encoded: Vec<_> = "004A\0".encode_utf16().flat_map(u16::to_le_bytes).collect();
        assert_eq!(parse_string(&encoded).unwrap(), Some("004A".to_owned()));
        assert_eq!(parse_string(&[0, 0]).unwrap(), None);
        for bytes in [
            vec![],
            vec![65],
            vec![65, 0],
            vec![0, 0xD8, 0, 0],
            vec![10, 0, 0, 0],
        ] {
            assert!(parse_string(&bytes).is_err());
        }
        assert_eq!(
            parse_date(&[29, 2, 0xE8, 7]).unwrap().as_deref(),
            Some("2024-02-29")
        );
        assert!(parse_date(&[29, 2, 0xE9, 7]).is_err());
        assert!(parse_date(&[1, 1]).is_err());
        assert_eq!(parse_date(&[0; 4]).unwrap(), None);
        for code in [ERROR_FILE_NOT_FOUND.0, ERROR_NO_SUCH_DEVICE.0] {
            assert!(stale_tag(&Error {
                op: "fixture",
                code,
                detail: String::new()
            }));
        }
        assert!(!stale_tag(&Error {
            op: "fixture",
            code: 5,
            detail: String::new()
        }));
    }
}
