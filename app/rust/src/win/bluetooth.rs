//! Owned by WP-06: runtime-loaded Bluetooth radio APIs.

use super::{Error, OwnedHandle, Result, dll, wide};
use windows::{
    Win32::{
        Devices::Bluetooth::{
            BLUETOOTH_FIND_RADIO_PARAMS, BLUETOOTH_RADIO_INFO, HBLUETOOTH_RADIO_FIND,
        },
        Foundation::{ERROR_NO_MORE_ITEMS, ERROR_PROC_NOT_FOUND, HANDLE},
    },
    core::{BOOL, HRESULT},
};

type FindFirst = unsafe extern "system" fn(
    *const BLUETOOTH_FIND_RADIO_PARAMS,
    *mut HANDLE,
) -> HBLUETOOTH_RADIO_FIND;
type FindNext = unsafe extern "system" fn(HBLUETOOTH_RADIO_FIND, *mut HANDLE) -> BOOL;
type FindClose = unsafe extern "system" fn(HBLUETOOTH_RADIO_FIND) -> BOOL;
type GetInfo = unsafe extern "system" fn(HANDLE, *mut BLUETOOTH_RADIO_INFO) -> u32;

/// One local radio's name and little-endian address from BluetoothGetRadioInfo.
#[derive(Debug)]
pub struct Radio {
    /// The name supplied by this local radio.
    pub name: String,
    /// This radio's six address bytes in Windows' little-endian order.
    pub address: [u8; 6],
}

impl Radio {
    /// Formats this radio's own address in the legacy uppercase colon notation.
    pub fn mac_address(&self) -> String {
        address_text(&self.address)
    }
}

/// Successful radio results and failures retained from the same enumeration.
#[derive(Debug, Default)]
pub struct RadioScan {
    /// Radios whose individual BluetoothGetRadioInfo call succeeded.
    pub radios: Vec<Radio>,
    /// Per-radio, enumeration, or search-handle cleanup failures.
    pub failures: Vec<Error>,
}

struct Api {
    _library: dll::Library,
    first: FindFirst,
    next: FindNext,
    close: FindClose,
    info: GetInfo,
}

impl Api {
    fn load() -> Result<Self> {
        let library = dll::load_system_dll("BluetoothApis.dll")?;
        let first = library
            .proc_address(c"BluetoothFindFirstRadio")
            .ok_or_else(|| missing_symbol("BluetoothFindFirstRadio"))?;
        let next = library
            .proc_address(c"BluetoothFindNextRadio")
            .ok_or_else(|| missing_symbol("BluetoothFindNextRadio"))?;
        let close = library
            .proc_address(c"BluetoothFindRadioClose")
            .ok_or_else(|| missing_symbol("BluetoothFindRadioClose"))?;
        let info = library
            .proc_address(c"BluetoothGetRadioInfo")
            .ok_or_else(|| missing_symbol("BluetoothGetRadioInfo"))?;
        // SAFETY: These symbols have the exact bluetoothapis.h system ABI above;
        // the library stays owned by Api throughout every function-pointer call.
        let (first, next, close, info) = unsafe {
            (
                std::mem::transmute::<unsafe extern "system" fn() -> isize, FindFirst>(first),
                std::mem::transmute::<unsafe extern "system" fn() -> isize, FindNext>(next),
                std::mem::transmute::<unsafe extern "system" fn() -> isize, FindClose>(close),
                std::mem::transmute::<unsafe extern "system" fn() -> isize, GetInfo>(info),
            )
        };
        Ok(Self {
            _library: library,
            first,
            next,
            close,
            info,
        })
    }
}

struct Search<'a> {
    handle: HBLUETOOTH_RADIO_FIND,
    api: &'a Api,
    failures: &'a mut Vec<Error>,
}

impl Drop for Search<'_> {
    fn drop(&mut self) {
        // SAFETY: This guard solely owns the search handle, and its API DLL is live.
        if !unsafe { (self.api.close)(self.handle) }.as_bool() {
            self.failures.push(Error::last("BluetoothFindRadioClose"));
        }
    }
}

fn missing_symbol(name: &str) -> Error {
    let mut error = status_error("GetProcAddress", ERROR_PROC_NOT_FOUND.0);
    error.detail.push_str(&format!(" ({name})"));
    error
}

fn status_error(op: &'static str, code: u32) -> Error {
    let mut error = Error::from_win(
        op,
        windows::core::Error::from_hresult(HRESULT::from_win32(code)),
    );
    // These APIs return Win32 status codes directly, rather than HRESULTs.
    error.code = code;
    error
}

/// Enumerates local radios without importing or performing discovery through optional DLLs.
pub fn radios() -> Result<RadioScan> {
    let api = Api::load()?;
    let params = BLUETOOTH_FIND_RADIO_PARAMS {
        dwSize: std::mem::size_of::<BLUETOOTH_FIND_RADIO_PARAMS>() as u32,
    };
    let mut handle = HANDLE::default();
    // SAFETY: params has the SDK size; handle is writable. Api owns the loaded function.
    let search_handle = unsafe { (api.first)(&params, &mut handle) };
    if search_handle.is_invalid() {
        let error = Error::last("BluetoothFindFirstRadio");
        if error.code == ERROR_NO_MORE_ITEMS.0 {
            return Ok(RadioScan::default());
        }
        return Err(error);
    }
    let mut scan = RadioScan::default();
    {
        let search = Search {
            handle: search_handle,
            api: &api,
            failures: &mut scan.failures,
        };
        loop {
            // SAFETY: The successful first/next call transferred one owned kernel radio handle.
            match unsafe { OwnedHandle::from_raw(handle) } {
                Ok(radio_handle) => {
                    let mut info = BLUETOOTH_RADIO_INFO {
                        dwSize: std::mem::size_of::<BLUETOOTH_RADIO_INFO>() as u32,
                        ..Default::default()
                    };
                    // SAFETY: The radio handle is live, info has the SDK size, and Api owns the DLL.
                    let status = unsafe { (api.info)(radio_handle.as_raw(), &mut info) };
                    if status == 0 {
                        // SAFETY: A successful GetRadioInfo initialized address; all byte patterns
                        // are valid for the six-byte member of the SDK address union.
                        let address = unsafe { info.address.Anonymous.rgBytes };
                        scan.radios.push(Radio {
                            name: wide::from_wide(&info.szName),
                            address,
                        });
                    } else {
                        search
                            .failures
                            .push(status_error("BluetoothGetRadioInfo", status));
                    }
                }
                Err(error) => search.failures.push(error),
            }
            handle = HANDLE::default();
            // SAFETY: Search owns the enumeration handle; handle is writable and Api is live.
            if !unsafe { (api.next)(search.handle, &mut handle) }.as_bool() {
                let error = Error::last("BluetoothFindNextRadio");
                if error.code != ERROR_NO_MORE_ITEMS.0 {
                    search.failures.push(error);
                }
                break;
            }
        }
    }
    Ok(scan)
}

fn address_text(bytes: &[u8]) -> String {
    bytes
        .iter()
        .rev()
        .map(|byte| format!("{byte:02X}"))
        .collect::<Vec<_>>()
        .join(":")
}

/// Reads the legacy global address without substituting any paired device's address.
pub fn legacy_registry_mac() -> Result<Option<String>> {
    // C# parity: Hardware/BluetoothInfo.cs:108-116. Reverse the whole value for
    // length >= 6, including surplus bytes; this quirk applies only to fallback.
    match super::registry::read_binary(
        r"SYSTEM\CurrentControlSet\Services\BTHPORT\Parameters\Bluetooth Host Controller",
        "LocalRadioAddress",
    ) {
        Ok(bytes) => Ok(registry_address(&bytes)),
        // C# parity: Hardware/BluetoothInfo.cs:110-113. Absent keys/values and a
        // value of another type are normal missing data, rather than exceptions.
        Err(error) if matches!(error.code, 2 | 3 | 1630) => Ok(None),
        Err(error) => Err(error),
    }
    // C# parity: Hardware/BluetoothInfo.cs:121-134. The paired Devices key never
    // contributes a local address, so opening it would not change any result.
}

fn registry_address(bytes: &[u8]) -> Option<String> {
    (bytes.len() >= 6).then(|| address_text(bytes))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn address_byte_order_and_legacy_surplus_bytes() {
        assert_eq!(
            address_text(&[0x91, 0x47, 0x2A, 0xF0, 0x8B, 0xC4]),
            "C4:8B:F0:2A:47:91"
        );
        assert_eq!(registry_address(&[0; 5]), None);
        assert_eq!(
            registry_address(&[0x91, 0x47, 0x2A, 0xF0, 0x8B, 0xC4, 0x01]),
            Some("01:C4:8B:F0:2A:47:91".into())
        );
    }
}
