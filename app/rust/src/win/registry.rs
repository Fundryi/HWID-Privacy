//! HKLM registry reads always use the 64-bit view.

use super::{Error, Result, wide};
use windows::Win32::Foundation::{ERROR_MORE_DATA, ERROR_NO_MORE_ITEMS, WIN32_ERROR};
use windows::Win32::System::Registry::{
    HKEY, HKEY_LOCAL_MACHINE, KEY_READ, KEY_WOW64_64KEY, REG_ROUTINE_FLAGS, RRF_RT_REG_BINARY,
    RRF_RT_REG_DWORD, RRF_RT_REG_EXPAND_SZ, RRF_RT_REG_QWORD, RRF_RT_REG_SZ, RegCloseKey,
    RegEnumKeyExW, RegGetValueW, RegOpenKeyExW,
};
use windows::core::{PCWSTR, PWSTR};

const MAX_VALUE_BYTES: usize = 16 * 1024 * 1024;

struct Key(HKEY);

impl Key {
    fn open(path: &str) -> Result<Self> {
        if path.is_empty() || path.contains('\0') {
            return Err(Error::msg(
                "RegOpenKeyExW",
                "expected a nonempty HKLM subkey path without NUL",
            ));
        }
        let path = wide::to_wide(path);
        let mut key = HKEY::default();
        // SAFETY: The path is terminated and key is writable; only read rights are requested.
        let status = unsafe {
            RegOpenKeyExW(
                HKEY_LOCAL_MACHINE,
                PCWSTR(path.as_ptr()),
                None,
                KEY_READ | KEY_WOW64_64KEY,
                &mut key,
            )
        };
        status
            .ok()
            .map_err(|_| status_error("RegOpenKeyExW", status))?;
        Ok(Self(key))
    }
}

impl Drop for Key {
    fn drop(&mut self) {
        // SAFETY: This is the unique owner of a key opened by RegOpenKeyExW.
        let status = unsafe { RegCloseKey(self.0) };
        if status.is_err() {
            eprintln!("{}", status_error("RegCloseKey", status));
        }
    }
}

fn status_error(op: &'static str, status: WIN32_ERROR) -> Error {
    Error {
        op,
        code: status.0,
        detail: windows::core::Error::from_hresult(status.to_hresult()).message(),
    }
}

fn read_value(path: &str, name: &str, flags: REG_ROUTINE_FLAGS) -> Result<Vec<u8>> {
    if name.contains('\0') {
        return Err(Error::msg(
            "RegGetValueW",
            "registry value name contains NUL",
        ));
    }
    let key = Key::open(path)?;
    let name = wide::to_wide(name);
    let mut capacity = 256;
    for _ in 0..4 {
        if capacity > MAX_VALUE_BYTES {
            return Err(Error::msg("RegGetValueW", "registry value exceeds 16 MiB"));
        }
        let mut buffer = Vec::<u8>::new();
        buffer
            .try_reserve_exact(capacity)
            .map_err(|e| Error::msg("RegGetValueW", format!("value allocation: {e}")))?;
        buffer.resize(capacity, 0);
        let mut length = capacity as u32;
        // SAFETY: The key, terminated name, writable buffer and byte-count pointer are live.
        // RegGetValue enforces the requested type and terminates/expands strings for us.
        let status = unsafe {
            RegGetValueW(
                key.0,
                PCWSTR::null(),
                PCWSTR(name.as_ptr()),
                flags,
                None,
                Some(buffer.as_mut_ptr().cast()),
                Some(&mut length),
            )
        };
        if status == ERROR_MORE_DATA {
            capacity = (length as usize).max(capacity * 2);
            continue;
        }
        status
            .ok()
            .map_err(|_| status_error("RegGetValueW", status))?;
        if length as usize > buffer.len() {
            return Err(Error::msg(
                "RegGetValueW",
                "returned size exceeds the registry buffer",
            ));
        }
        buffer.truncate(length as usize);
        return Ok(buffer);
    }
    Err(Error::msg(
        "RegGetValueW",
        "registry value kept growing during the read",
    ))
}

/// Reads a REG_SZ or REG_EXPAND_SZ value from HKLM's 64-bit view.
pub fn read_string(path: &str, name: &str) -> Result<String> {
    // C# parity: Hardware/NetworkInfo.cs:174
    // Default GetValue expands REG_EXPAND_SZ, while REG_SZ stays literal.
    let bytes = read_value(path, name, RRF_RT_REG_SZ | RRF_RT_REG_EXPAND_SZ)?;
    if bytes.len() % 2 != 0 {
        return Err(Error::msg(
            "RegGetValueW",
            "registry string has an odd UTF-16 byte length",
        ));
    }
    let units: Vec<u16> = bytes
        .as_chunks::<2>()
        .0
        .iter()
        .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
        .collect();
    // Expansion can shorten the string while the returned byte count still includes old data.
    Ok(wide::from_wide(&units))
}
/// Reads a REG_DWORD value from HKLM's 64-bit view.
pub fn read_dword(path: &str, name: &str) -> Result<u32> {
    let bytes = read_value(path, name, RRF_RT_REG_DWORD)?;
    let value = bytes
        .try_into()
        .map_err(|_| Error::msg("RegGetValueW", "REG_DWORD must contain exactly four bytes"))?;
    Ok(u32::from_le_bytes(value))
}
/// Reads a REG_QWORD value from HKLM's 64-bit view.
pub fn read_qword(path: &str, name: &str) -> Result<u64> {
    let bytes = read_value(path, name, RRF_RT_REG_QWORD)?;
    let value = bytes
        .try_into()
        .map_err(|_| Error::msg("RegGetValueW", "REG_QWORD must contain exactly eight bytes"))?;
    Ok(u64::from_le_bytes(value))
}
/// Reads a REG_BINARY value from HKLM's 64-bit view.
pub fn read_binary(path: &str, name: &str) -> Result<Vec<u8>> {
    read_value(path, name, RRF_RT_REG_BINARY)
}
/// Lists immediate subkey names under a key in HKLM's 64-bit view.
pub fn subkeys(path: &str) -> Result<Vec<String>> {
    let key = Key::open(path)?;
    let mut names = Vec::new();
    let mut buffer = vec![0_u16; 256];
    let mut index = 0_u32;
    loop {
        let mut length = buffer.len() as u32;
        // C# parity: Hardware/MonitorInfo.cs:117
        // SAFETY: The key is live, buffer is writable for length units; other outputs are omitted.
        let status = unsafe {
            RegEnumKeyExW(
                key.0,
                index,
                Some(PWSTR(buffer.as_mut_ptr())),
                &mut length,
                None,
                None,
                None,
                None,
            )
        };
        if status == ERROR_NO_MORE_ITEMS {
            return Ok(names);
        }
        if status == ERROR_MORE_DATA && buffer.len() < 32768 {
            buffer.resize(buffer.len() * 2, 0);
            continue;
        }
        status
            .ok()
            .map_err(|_| status_error("RegEnumKeyExW", status))?;
        if length as usize > buffer.len() {
            return Err(Error::msg(
                "RegEnumKeyExW",
                "returned subkey length exceeds the buffer",
            ));
        }
        names.push(String::from_utf16_lossy(&buffer[..length as usize]));
        index = index
            .checked_add(1)
            .ok_or_else(|| Error::msg("RegEnumKeyExW", "subkey index overflow"))?;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use windows::Win32::Foundation::ERROR_FILE_NOT_FOUND;

    const CURRENT_VERSION: &str = r"SOFTWARE\Microsoft\Windows NT\CurrentVersion";

    #[test]
    fn reads_product_name_in_the_64_bit_view_without_admin() {
        let product =
            read_string(CURRENT_VERSION, "ProductName").expect("Windows check should succeed");
        assert!(!product.is_empty());
        assert!(!product.contains('\0'));
        assert!(
            read_dword(CURRENT_VERSION, "InstallDate").expect("Windows check should succeed") > 0
        );
        assert!(
            read_qword(CURRENT_VERSION, "InstallTime").expect("Windows check should succeed") > 0
        );
        assert_eq!(
            read_binary(
                r"SOFTWARE\Microsoft\Windows NT\CurrentVersion\Time Zones\UTC",
                "TZI",
            )
            .expect("Windows timezone registry data must be readable")
            .len(),
            44,
        );
        let expanded = read_string(
            r"SYSTEM\CurrentControlSet\Control\Session Manager\Environment",
            "ComSpec",
        )
        .expect("REG_EXPAND_SZ must expand environment variables");
        assert_eq!(
            expanded.to_uppercase(),
            std::env::var("ComSpec")
                .expect("Windows sets ComSpec")
                .to_uppercase(),
        );
        println!("Expanded ComSpec: {expanded}");
        assert!(
            subkeys(r"SOFTWARE\Microsoft\Windows NT")
                .expect("Windows check should succeed")
                .iter()
                .any(|name| name == "CurrentVersion")
        );
    }

    #[test]
    fn missing_key_value_and_wrong_type_return_errors() {
        let missing_key = read_string(r"SOFTWARE\HWIDChecker-Phase1-Missing-6f28e90c", "Value")
            .expect_err("this check must return an error");
        let missing_value = read_string(CURRENT_VERSION, "HWIDChecker-Phase1-Missing-6f28e90c")
            .expect_err("this check must return an error");
        assert_eq!(missing_key.code, ERROR_FILE_NOT_FOUND.0);
        assert_eq!(missing_value.code, ERROR_FILE_NOT_FOUND.0);
        assert!(!missing_key.detail.is_empty());
        assert!(read_dword(CURRENT_VERSION, "ProductName").is_err());
        assert!(read_qword(CURRENT_VERSION, "ProductName").is_err());
        assert!(read_binary(CURRENT_VERSION, "ProductName").is_err());
        assert!(read_string(CURRENT_VERSION, "ProductName\0ignored").is_err());
    }
}
