//! Read-only battery interfaces and independently queried identity fields.

use super::{Error, OwnedHandle, Result, ioctl};
use std::{
    sync::{Arc, mpsc},
    time::{Duration, Instant},
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
    let deadline = Instant::now() + Duration::from_secs(15);
    for path in bounded("battery interfaces", Duration::from_secs(2), interfaces)? {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            scan.failures.push(Error::msg(
                "battery collection",
                "timeout: 15-second budget expired",
            ));
            break;
        }
        match path.and_then(|path| {
            bounded(
                "battery snapshot",
                remaining.min(Duration::from_secs(8)),
                move || {
                    let mut failures = Vec::new();
                    let result = read_battery(&path, &mut failures);
                    Ok((result, failures))
                },
            )
        }) {
            Ok((result, failures)) => {
                scan.failures.extend(failures);
                match result {
                    Ok(Some(battery)) => scan.batteries.push(battery),
                    Ok(None) => scan
                        .failures
                        .push(Error::msg("battery tag", "absent: empty battery bay")),
                    Err(error) => scan.batteries.push(failed_battery(error)),
                }
            }
            Err(error) => scan.batteries.push(failed_battery(error)),
        }
    }
    Ok(scan)
}

fn failed_battery(error: Error) -> Battery {
    Battery {
        fields: vec![Field {
            label: "Battery Information",
            identity: false,
            value: Err(error),
        }],
    }
}

fn bounded<T: Send + 'static>(
    op: &'static str,
    wait: Duration,
    read: impl FnOnce() -> Result<T> + Send + 'static,
) -> Result<T> {
    let (send, receive) = mpsc::sync_channel(1);
    std::thread::Builder::new()
        .name(op.to_owned())
        .spawn(move || {
            let result = super::catch_panic(read)
                .unwrap_or_else(|_| Err(Error::msg(op, "malformed: query worker panicked")));
            let _ = send.send(result);
        })
        .map_err(|_| Error::msg(op, "unavailable: failed to start query worker"))?;
    receive.recv_timeout(wait).map_err(|error| {
        Error::msg(
            op,
            match error {
                mpsc::RecvTimeoutError::Timeout => "timeout: caller budget expired",
                mpsc::RecvTimeoutError::Disconnected => "unavailable: query worker disconnected",
            },
        )
    })?
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
        return parse_interfaces(&buffer);
    }
    Err(Error::msg(
        "battery interfaces",
        "interface list kept growing",
    ))
}

fn parse_interfaces(buffer: &[u16]) -> Result<Vec<Result<String>>> {
    if buffer.is_empty() || buffer.len() > 65536 || buffer.last() != Some(&0) {
        return Err(Error::msg("battery interfaces", "malformed interface list"));
    }
    let mut paths = Vec::new();
    let mut remaining = buffer;
    while let Some(&first) = remaining.first() {
        if first == 0 {
            if remaining.iter().any(|unit| *unit != 0) {
                return Err(Error::msg(
                    "battery interfaces",
                    "malformed trailing interface data",
                ));
            }
            break;
        }
        if paths.len() == 64 {
            return Err(Error::msg(
                "battery interfaces",
                "malformed: interface count exceeds 64",
            ));
        }
        let end = remaining
            .iter()
            .position(|unit| *unit == 0)
            .ok_or_else(|| Error::msg("battery interfaces", "malformed unterminated path"))?;
        let units = remaining
            .get(..end)
            .ok_or_else(|| Error::msg("battery interfaces", "malformed path bounds"))?;
        paths.push(
            String::from_utf16(units)
                .map_err(|_| Error::msg("battery interfaces", "malformed UTF-16 path"))
                .and_then(|path| {
                    if path.chars().any(char::is_control) {
                        Err(Error::msg(
                            "battery interfaces",
                            "malformed: control character in path",
                        ))
                    } else {
                        Ok(path)
                    }
                }),
        );
        remaining = remaining
            .get(end + 1..)
            .filter(|tail| !tail.is_empty())
            .ok_or_else(|| Error::msg("battery interfaces", "malformed missing list terminator"))?;
    }
    paths.sort_by_key(|path| path.as_ref().ok().map(|path| path.to_ascii_lowercase()));
    if paths.windows(2).any(|pair| {
        pair[0].as_ref().ok().is_some_and(|a| {
            pair[1]
                .as_ref()
                .ok()
                .is_some_and(|b| a.eq_ignore_ascii_case(b))
        })
    }) {
        return Err(Error::msg(
            "battery interfaces",
            "ambiguous duplicate interface paths",
        ));
    }
    Ok(paths)
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
            match verify_snapshot(tag, fields, query_tag(&handle)) {
                Ok(battery) => return Ok(Some(battery)),
                Err(error) if stale_tag(&error) => stale = Some(error),
                Err(error) => {
                    // An unverified tag cannot prove that the fields belong to one pack.
                    failures.push(error.clone());
                    return Ok(Some(Battery {
                        fields: vec![Field {
                            label: "Battery Tag Verification",
                            identity: false,
                            value: Err(error),
                        }],
                    }));
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
    Err(Error::msg(
        "battery tag",
        "ambiguous: no verified battery snapshot",
    ))
}

fn verify_snapshot(
    tag: u32,
    fields: Vec<Field>,
    final_tag: Result<Option<u32>>,
) -> Result<Battery> {
    match final_tag {
        Ok(Some(current)) if tag != 0 && current == tag => Ok(Battery { fields }),
        Ok(_) => Err(Error {
            op: "battery tag",
            code: ERROR_NO_SUCH_DEVICE.0,
            detail: "ambiguous: battery changed during queries".into(),
        }),
        Err(error) => Err(error),
    }
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
    // As in storage::bounded, the worker owns all kernel-visible memory until return.
    // Abandoning the caller's 500-ms wait never frees an in-flight request's buffers.
    bounded("battery query", WAIT, move || {
        let mut output = vec![0_u8; capacity];
        let mut returned = 0;
        // SAFETY: The shared handle, input, output and count remain live through this
        // synchronous read-only IOCTL; no OVERLAPPED or driver-write request is used.
        unsafe {
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
        .map_err(|e| {
            let mut error = Error::from_win("battery query", e);
            error.detail = match error.code {
                2 | 433 => "absent: battery tag became stale",
                5 => "access-denied: battery query",
                1 | 50 => "unsupported: information level",
                _ => "unavailable: battery query failed",
            }
            .into();
            error
        })
        .and_then(|()| {
            if returned as usize > output.len() {
                return Err(Error::msg("battery query", "malformed returned length"));
            }
            output.truncate(returned as usize);
            Ok(output)
        })
    })
}

fn parse_string(bytes: &[u8]) -> Result<Option<String>> {
    if bytes.is_empty() || bytes.len() > MAX_STRING_BYTES || !bytes.len().is_multiple_of(2) {
        return Err(Error::msg("battery string", "malformed UTF-16 byte count"));
    }
    let units: Vec<_> = bytes
        .as_chunks::<2>()
        .0
        .iter()
        .map(|pair| u16::from_le_bytes(*pair))
        .collect();
    let end = units
        .iter()
        .position(|unit| *unit == 0)
        .ok_or_else(|| Error::msg("battery string", "unterminated UTF-16 string"))?;
    if units
        .get(end..)
        .is_some_and(|tail| tail.iter().any(|unit| *unit != 0))
    {
        return Err(Error::msg(
            "battery string",
            "malformed: nonzero data after string terminator",
        ));
    }
    let value = String::from_utf16(
        units
            .get(..end)
            .ok_or_else(|| Error::msg("battery string", "malformed string bounds"))?,
    )
    .map_err(|_| Error::msg("battery string", "malformed UTF-16 string"))?;
    if value.chars().any(char::is_control) {
        return Err(Error::msg("battery string", "control character in string"));
    }
    if matches!(
        value.trim().to_ascii_lowercase().as_str(),
        "unknown"
            | "none"
            | "n/a"
            | "default string"
            | "to be filled by o.e.m."
            | "to be filled by oem"
    ) {
        return Err(Error::msg(
            "battery string",
            "placeholder: battery field omitted",
        ));
    }
    Ok((!value.trim().is_empty()).then_some(value))
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
    fn interfaces_and_snapshot_verification_fail_closed() {
        let paths: Vec<_> = "\\\\?\\Battery-B\0\\\\?\\Battery-A\0\0"
            .encode_utf16()
            .collect();
        let parsed = parse_interfaces(&paths).unwrap();
        assert_eq!(parsed.len(), 2);
        assert!(parsed[0].as_ref().unwrap().ends_with("Battery-A"));
        assert!(parse_interfaces(&[0]).unwrap().is_empty());
        for bad in ["A\0", "A\0A\0\0", "A\0\0B\0\0"] {
            assert!(parse_interfaces(&bad.encode_utf16().collect::<Vec<_>>()).is_err());
        }
        assert!(
            parse_interfaces(
                &"A\0"
                    .repeat(65)
                    .encode_utf16()
                    .chain([0])
                    .collect::<Vec<_>>()
            )
            .is_err()
        );
        let fields = || {
            vec![
                Field {
                    label: "Battery Serial",
                    identity: true,
                    value: Ok(Some("BAT2408G7192".into())),
                },
                Field {
                    label: "Battery Name",
                    identity: false,
                    value: Err(Error::msg("fixture", "unsupported: field")),
                },
            ]
        };
        assert_eq!(
            verify_snapshot(17, fields(), Ok(Some(17)))
                .unwrap()
                .fields
                .len(),
            2
        );
        for current in [
            Ok(None),
            Ok(Some(18)),
            Err(Error {
                op: "fixture",
                code: ERROR_FILE_NOT_FOUND.0,
                detail: "absent".into(),
            }),
            Err(Error {
                op: "fixture",
                code: 5,
                detail: "access-denied".into(),
            }),
        ] {
            assert!(verify_snapshot(17, fields(), current).is_err());
        }
        assert!(
            bounded::<()>("fixture", Duration::from_millis(1), || {
                std::thread::sleep(Duration::from_millis(20));
                Ok(())
            })
            .unwrap_err()
            .detail
            .contains("timeout")
        );
        assert!(
            bounded::<()>("fixture", Duration::from_secs(1), || panic!(
                "fixture private data"
            ))
            .unwrap_err()
            .detail
            .contains("panicked")
        );
    }

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
