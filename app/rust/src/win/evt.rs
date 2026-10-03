//! Owned by WP-12: event log APIs.

use std::{mem::size_of, path::PathBuf};
use windows::{
    Win32::System::EventLog::{
        EVT_CHANNEL_CONFIG_PROPERTY_ID, EVT_HANDLE, EVT_VARIANT, EVT_VARIANT_0,
        EvtChannelConfigEnabled, EvtChannelConfigType, EvtClearLog, EvtClose,
        EvtGetChannelConfigProperty, EvtNextChannelPath, EvtOpenChannelConfig, EvtOpenChannelEnum,
        EvtSaveChannelConfig, EvtSetChannelConfigProperty, EvtVarTypeBoolean, EvtVarTypeUInt32,
    },
    core::{BOOL, PCWSTR},
};

use super::{Error, Result, process::Cancel, wide};

// The frozen feature list omits Win32_Globalization. Ordinal comparison is in Kernel32.
#[link(name = "kernel32")]
// SAFETY: This declaration matches the Kernel32 UTF-16 ordinal-comparison ABI.
unsafe extern "system" {
    fn CompareStringOrdinal(
        left: *const u16,
        left_len: i32,
        right: *const u16,
        right_len: i32,
        ignore_case: BOOL,
    ) -> i32;
}

/// Sorts channel names with Windows ordinal case-insensitive UTF-16 comparison.
pub fn sort_channel_names(names: &mut [String]) -> Result<()> {
    let mut failure = None;
    names.sort_by(|left, right| {
        let left = ordinal_wide(left);
        let right = ordinal_wide(right);
        // SAFETY: Both strings are NUL-terminated live UTF-16 buffers; -1 requests those full strings.
        match unsafe { CompareStringOrdinal(left.as_ptr(), -1, right.as_ptr(), -1, BOOL(1)) } {
            1 => std::cmp::Ordering::Less,
            2 => std::cmp::Ordering::Equal,
            3 => std::cmp::Ordering::Greater,
            _ => {
                failure = Some(Error::last("CompareStringOrdinal"));
                std::cmp::Ordering::Equal
            }
        }
    });
    if let Some(error) = failure {
        Err(error)
    } else {
        Ok(())
    }
}

fn ordinal_wide(value: &str) -> Vec<u16> {
    // Windows ordinal casing leaves surrogate pairs unchanged, unlike .NET.
    // Fold supplementary scalars first; BMP casing still uses the OS's non-expanding comparison.
    let value: String = value
        .chars()
        .map(|ch| {
            if ch as u32 <= 0xffff {
                return ch;
            }
            let mut upper = ch.to_uppercase();
            match (upper.next(), upper.next()) {
                (Some(upper), None) if !upper.is_ascii() => upper,
                _ => ch,
            }
        })
        .collect();
    wide::to_wide(&value)
}

struct EventHandle(EVT_HANDLE);
impl Drop for EventHandle {
    fn drop(&mut self) {
        // SAFETY: This is the sole owner of a live event handle returned by wevtapi.
        if let Err(error) = unsafe { EvtClose(self.0) } {
            eprintln!("{}", Error::from_win("EvtClose", error));
        }
    }
}

/// Holds discovered channels and any error that ended a partial enumeration.
pub struct Channels {
    /// Channel names in the native enumeration order.
    pub names: Vec<String>,
    /// The failure after which enumeration stopped, if any.
    pub failure: Option<Error>,
}

/// Enumerates local event channels, preserving partial results on failure.
pub fn enumerate_channels(cancel: &Cancel) -> Result<Channels> {
    // SAFETY: The local session and reserved flags are zero; the handle gains one owner.
    let handle = EventHandle(
        unsafe { EvtOpenChannelEnum(None, 0) }.map_err(|_| Error::last("EvtOpenChannelEnum"))?,
    );
    let mut channels = Channels {
        names: Vec::new(),
        failure: None,
    };
    // C# parity: EventLogApi.cs:184 (initial channel path buffer).
    let mut buffer = vec![0_u16; 512];
    while !cancel.is_cancelled() {
        let mut used = 0;
        // SAFETY: The owning handle, writable slice, and used count remain live.
        let result = unsafe { EvtNextChannelPath(handle.0, Some(&mut buffer), &mut used) };
        if result.is_err() {
            let error = Error::last("EvtNextChannelPath");
            if error.code == 259 {
                break;
            }
            if error.code == 122 && used > 0 && used <= 32768 {
                buffer.resize(used as usize, 0);
                // C# parity: EventLogApi.cs:200-209 (resize and retry once).
                // SAFETY: The resized writable slice and live handle match the advertised capacity.
                if unsafe { EvtNextChannelPath(handle.0, Some(&mut buffer), &mut used) }.is_err() {
                    channels.failure = Some(Error::last("EvtNextChannelPath"));
                    break;
                }
            } else {
                channels.failure = Some(error);
                break;
            }
        }
        if used == 0 || used as usize > buffer.len() || buffer[used as usize - 1] != 0 {
            channels.failure = Some(Error::msg(
                "EvtNextChannelPath",
                "invalid channel path length",
            ));
            break;
        }
        channels
            .names
            .push(wide::from_wide(&buffer[..used as usize - 1]));
    }
    Ok(channels)
}

fn channel_path(name: &str) -> Result<Vec<u16>> {
    if name.is_empty() || name.contains('\0') {
        return Err(Error::msg(
            "EvtOpenChannelConfig",
            "empty or NUL-containing channel name",
        ));
    }
    Ok(wide::to_wide(name))
}

fn config(name: &str) -> Result<EventHandle> {
    let name = channel_path(name)?;
    // SAFETY: The NUL-terminated channel path lives through the local configuration open.
    unsafe { EvtOpenChannelConfig(None, PCWSTR(name.as_ptr()), 0) }
        .map(EventHandle)
        .map_err(|_| Error::last("EvtOpenChannelConfig"))
}

fn property(name: &str, id: EVT_CHANNEL_CONFIG_PROPERTY_ID, kind: u32) -> Result<EVT_VARIANT> {
    let handle = config(name)?;
    let mut value = EVT_VARIANT::default();
    let mut used = 0;
    // SAFETY: The aligned SDK variant is writable for exactly its size; the config stays owned.
    unsafe {
        EvtGetChannelConfigProperty(
            handle.0,
            id,
            0,
            size_of::<EVT_VARIANT>() as u32,
            Some(&mut value),
            &mut used,
        )
    }
    .map_err(|_| Error::last("EvtGetChannelConfigProperty"))?;
    validate_variant(&value, used, kind)?;
    Ok(value)
}

fn validate_variant(value: &EVT_VARIANT, used: u32, kind: u32) -> Result<()> {
    if used != size_of::<EVT_VARIANT>() as u32 || value.Type != kind || value.Count != 0 {
        return Err(Error::msg(
            "EvtGetChannelConfigProperty",
            format!(
                "expected scalar type {kind}, got type {} count {} size {used}",
                value.Type, value.Count
            ),
        ));
    }
    Ok(())
}

/// Reads the enabled flag only from a validated scalar Boolean variant.
pub fn is_channel_enabled(name: &str) -> Result<bool> {
    let value = property(name, EvtChannelConfigEnabled, EvtVarTypeBoolean.0 as u32)?;
    // SAFETY: property verified the complete variant's scalar Boolean tag.
    Ok(unsafe { value.Anonymous.BooleanVal }.as_bool())
}

/// Reads the Admin/Operational/Analytic/Debug channel type as a scalar UInt32.
pub fn channel_type(name: &str) -> Result<u32> {
    let value = property(name, EvtChannelConfigType, EvtVarTypeUInt32.0 as u32)?;
    // SAFETY: property verified the complete variant's scalar UInt32 tag.
    Ok(unsafe { value.Anonymous.UInt32Val })
}

/// Clears a local channel; the caller must apply the shared destructive guard.
pub fn clear_log(name: &str) -> Result<()> {
    let name = channel_path(name)?;
    // SAFETY: The live NUL-terminated path names a local channel; no backup path is supplied.
    unsafe { EvtClearLog(None, PCWSTR(name.as_ptr()), PCWSTR::null(), 0) }
        .map_err(|_| Error::last("EvtClearLog"))
}

/// Saves a channel's enabled state; the caller must apply the shared destructive guard.
pub fn set_channel_enabled(name: &str, enabled: bool) -> Result<()> {
    let handle = config(name)?;
    let value = EVT_VARIANT {
        Anonymous: EVT_VARIANT_0 {
            BooleanVal: BOOL::from(enabled),
        },
        Count: 0,
        Type: EvtVarTypeBoolean.0 as u32,
    };
    // SAFETY: A complete scalar Boolean variant and the owned config are live through the call.
    unsafe { EvtSetChannelConfigProperty(handle.0, EvtChannelConfigEnabled, 0, &value) }
        .map_err(|_| Error::last("EvtSetChannelConfigProperty"))?;
    // SAFETY: The same config handle is live and reserved flags are zero.
    unsafe { EvtSaveChannelConfig(handle.0, 0) }.map_err(|_| Error::last("EvtSaveChannelConfig"))
}

// The frozen feature list omits Win32_System_WindowsProgramming; this is an OS-only API.
#[link(name = "advapi32")]
// SAFETY: This declaration matches Advapi32's writable UTF-16 GetUserNameW ABI.
unsafe extern "system" {
    fn GetUserNameW(buffer: *mut u16, size: *mut u32) -> BOOL;
}

/// Gets the current Windows user name for the legacy icacls grant.
pub fn user_name() -> Result<String> {
    let mut buffer = [0_u16; 257];
    let mut size = buffer.len() as u32;
    // SAFETY: buffer and size are writable; capacity covers UNLEN plus its NUL.
    if !unsafe { GetUserNameW(buffer.as_mut_ptr(), &mut size) }.as_bool() {
        return Err(Error::last("GetUserNameW"));
    }
    Ok(wide::from_wide(&buffer))
}

/// Creates the empty temporary file used by the C# export-and-clear fallback.
pub fn temporary_file() -> Result<PathBuf> {
    use std::os::windows::ffi::{OsStrExt, OsStringExt};
    use windows::Win32::Storage::FileSystem::GetTempFileNameW;
    let mut directory: Vec<u16> = std::env::temp_dir().as_os_str().encode_wide().collect();
    directory.push(0);
    let prefix = wide::to_wide("tmp");
    let mut file = [0_u16; 260];
    // SAFETY: NUL-terminated directory/prefix and a MAX_PATH output buffer are live; zero creates a file.
    if unsafe {
        GetTempFileNameW(
            PCWSTR(directory.as_ptr()),
            PCWSTR(prefix.as_ptr()),
            0,
            &mut file,
        )
    } == 0
    {
        return Err(Error::last("GetTempFileNameW"));
    }
    let end = file.iter().position(|c| *c == 0).unwrap_or(file.len());
    Ok(std::ffi::OsString::from_wide(&file[..end]).into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn supplementary_case_channel_sort_matches_dotnet() {
        // .NET OrdinalIgnoreCase orders these fabricated channel names by the suffix.
        // CompareStringOrdinal alone does not case-fold the Deseret surrogate pair.
        let mut names = vec!["𐐀/z".to_owned(), "𐐨/a".to_owned()];
        sort_channel_names(&mut names).expect("valid ordinal comparison");
        assert_eq!(names, ["𐐨/a", "𐐀/z"]);
    }

    #[test]
    fn rejects_wrong_array_and_truncated_enabled_variants() {
        assert_eq!(size_of::<EVT_VARIANT>(), 16);
        assert_eq!(std::mem::offset_of!(EVT_VARIANT, Count), 8);
        assert_eq!(std::mem::offset_of!(EVT_VARIANT, Type), 12);
        for (kind, count, used) in [(0, 0, 16), (13 | 128, 1, 16), (13, 1, 16), (13, 0, 8)] {
            let value = EVT_VARIANT {
                Type: kind,
                Count: count,
                ..Default::default()
            };
            assert!(validate_variant(&value, used, 13).is_err());
        }
        assert!(
            validate_variant(
                &EVT_VARIANT {
                    Type: 13,
                    ..Default::default()
                },
                16,
                13
            )
            .is_ok()
        );
    }
}
