//! SetupAPI snapshots retain ownership while borrowed devices are inspected.

use super::{Error, Result, wide};
use std::collections::{HashMap, HashSet};
use windows::Win32::Devices::DeviceAndDriverInstallation::{
    CM_LOCATE_DEVNODE_NORMAL, CM_Locate_DevNodeW, CM_MapCrToWin32Err, CR_NO_SUCH_DEVNODE,
    CR_SUCCESS, DIGCF_ALLCLASSES, DIGCF_PRESENT, HDEVINFO, SETUP_DI_GET_CLASS_DEVS_FLAGS,
    SETUP_DI_REGISTRY_PROPERTY, SP_DEVINFO_DATA, SPDRP_HARDWAREID, SetupDiDestroyDeviceInfoList,
    SetupDiEnumDeviceInfo, SetupDiGetClassDevsW, SetupDiGetDeviceInstanceIdW,
    SetupDiGetDeviceRegistryPropertyW, SetupDiRemoveDevice,
};
use windows::Win32::Foundation::{
    ERROR_GEN_FAILURE, ERROR_INSUFFICIENT_BUFFER, ERROR_NO_MORE_ITEMS,
};
use windows::Win32::System::Registry::{REG_EXPAND_SZ, REG_MULTI_SZ, REG_SZ};
use windows::core::{HRESULT, PCWSTR};

pub struct DevInfoSet {
    handle: HDEVINFO,
}
pub struct Device<'a> {
    set: &'a DevInfoSet,
    data: SP_DEVINFO_DATA,
}
pub struct RawProperty {
    pub kind: u32,
    pub required_size: u32,
    pub bytes: Vec<u8>,
}

impl DevInfoSet {
    /// Owns a snapshot containing all presently connected device classes.
    pub fn enum_present_all() -> Result<Self> {
        Self::open(DIGCF_ALLCLASSES | DIGCF_PRESENT)
    }
    /// Owns a snapshot containing present and historical devices of every class.
    pub fn enum_all() -> Result<Self> {
        Self::open(DIGCF_ALLCLASSES)
    }

    fn open(flags: SETUP_DI_GET_CLASS_DEVS_FLAGS) -> Result<Self> {
        // SAFETY: ALLCLASSES permits a null class GUID; no enumerator or parent is supplied.
        let handle = unsafe { SetupDiGetClassDevsW(None, PCWSTR::null(), None, flags) }
            .map_err(|e| Error::from_win("SetupDiGetClassDevsW", e))?;
        Ok(Self { handle })
    }
    /// Enumerates devices borrowing this snapshot so the HDEVINFO stays alive.
    pub fn devices(&self) -> Result<Vec<Device<'_>>> {
        let mut devices = Vec::new();
        let mut index = 0_u32;
        loop {
            let mut data = SP_DEVINFO_DATA {
                cbSize: std::mem::size_of::<SP_DEVINFO_DATA>() as u32,
                ..Default::default()
            };
            // SAFETY: The set remains owned by self and data has the SDK-required size.
            match unsafe { SetupDiEnumDeviceInfo(self.handle, index, &mut data) } {
                Ok(()) => devices.push(Device { set: self, data }),
                Err(error) if error.code() == HRESULT::from_win32(ERROR_NO_MORE_ITEMS.0) => {
                    return Ok(devices);
                }
                Err(error) => return Err(Error::from_win("SetupDiEnumDeviceInfo", error)),
            }
            index = index
                .checked_add(1)
                .ok_or_else(|| Error::msg("SetupDiEnumDeviceInfo", "device index overflow"))?;
        }
    }
    /// Borrows the native set handle for guarded device removal.
    pub fn as_raw(&self) -> HDEVINFO {
        self.handle
    }

    /// Removes copied device data belonging to this live snapshot after the caller's guard.
    pub fn remove_device(&self, data: &SP_DEVINFO_DATA) -> Result<()> {
        // The cleaner must invoke this only through clean::destructive after its presence check.
        // C# parity: app/src/Services/DeviceCleaningService.cs:171-175
        let mut data = *data;
        // SAFETY: self owns the live set; the SDK validates the copied device data against it.
        if unsafe { SetupDiRemoveDevice(self.handle, &mut data) }.as_bool() {
            Ok(())
        } else {
            Err(Error::last("SetupDiRemoveDevice"))
        }
    }
}

impl Drop for DevInfoSet {
    fn drop(&mut self) {
        // SAFETY: Only open creates this uniquely owned set; borrowed devices cannot outlive it.
        if let Err(error) = unsafe { SetupDiDestroyDeviceInfoList(self.handle) } {
            eprintln!("{}", Error::from_win("SetupDiDestroyDeviceInfoList", error));
        }
    }
}

impl Device<'_> {
    /// Reads the exact device instance ID from this snapshot.
    pub fn instance_id(&self) -> Result<String> {
        for _ in 0..3 {
            let mut required = 0;
            // SAFETY: The device borrows its live set; None queries the UTF-16 unit count.
            let query = unsafe {
                SetupDiGetDeviceInstanceIdW(self.set.handle, &self.data, None, Some(&mut required))
            };
            if let Err(error) = query
                && error.code() != HRESULT::from_win32(ERROR_INSUFFICIENT_BUFFER.0)
            {
                return Err(Error::from_win("SetupDiGetDeviceInstanceIdW", error));
            }
            if required == 0 {
                return Err(Error::msg(
                    "SetupDiGetDeviceInstanceIdW",
                    "empty instance ID size",
                ));
            }
            let mut buffer = Vec::new();
            buffer
                .try_reserve_exact(required as usize)
                .map_err(|e| Error::msg("SetupDiGetDeviceInstanceIdW", e.to_string()))?;
            buffer.resize(required as usize, 0);
            // SAFETY: buffer has the queried count, fits u32, and data belongs to the live set.
            match unsafe {
                SetupDiGetDeviceInstanceIdW(
                    self.set.handle,
                    &self.data,
                    Some(&mut buffer),
                    Some(&mut required),
                )
            } {
                Ok(()) if required as usize <= buffer.len() => {
                    return Ok(wide::from_wide(&buffer[..required as usize]));
                }
                Ok(()) => {
                    return Err(Error::msg(
                        "SetupDiGetDeviceInstanceIdW",
                        "invalid output size",
                    ));
                }
                Err(error) if error.code() == HRESULT::from_win32(ERROR_INSUFFICIENT_BUFFER.0) => {
                    // The instance ID grew after the size query; retry with a fresh size.
                    continue;
                }
                Err(error) => return Err(Error::from_win("SetupDiGetDeviceInstanceIdW", error)),
            }
        }
        Err(Error::msg(
            "SetupDiGetDeviceInstanceIdW",
            "instance ID kept growing during three reads",
        ))
    }
    /// Reads a UTF-16 registry property as a single string.
    pub fn property_string(&self, property: SETUP_DI_REGISTRY_PROPERTY) -> Result<String> {
        let raw = self.property(property)?;
        if raw.kind != REG_SZ.0 && raw.kind != REG_EXPAND_SZ.0 {
            return Err(Error::msg(
                "SetupAPI string property",
                "property is not REG_SZ or REG_EXPAND_SZ",
            ));
        }
        Ok(wide::from_wide(&property_units(&raw.bytes)?))
    }
    /// Reads a REG_MULTI_SZ registry property without joining its elements.
    pub fn property_multi_sz(&self, property: SETUP_DI_REGISTRY_PROPERTY) -> Result<Vec<String>> {
        let raw = self.property(property)?;
        if raw.kind != REG_MULTI_SZ.0 {
            return Err(Error::msg(
                "SetupAPI multi-sz property",
                "property is not REG_MULTI_SZ",
            ));
        }
        Ok(property_units(&raw.bytes)?
            .split(|unit| *unit == 0)
            .take_while(|string| !string.is_empty())
            .map(String::from_utf16_lossy)
            .collect())
    }

    fn property(&self, property: SETUP_DI_REGISTRY_PROPERTY) -> Result<RawProperty> {
        for _ in 0..3 {
            let mut required = 0;
            let mut kind = 0;
            // SAFETY: The device borrows its live set; no buffer is supplied for the size query.
            let query = unsafe {
                SetupDiGetDeviceRegistryPropertyW(
                    self.set.handle,
                    &self.data,
                    property,
                    Some(&mut kind),
                    None,
                    Some(&mut required),
                )
            };
            if let Err(error) = query
                && error.code() != HRESULT::from_win32(ERROR_INSUFFICIENT_BUFFER.0)
            {
                return Err(Error::from_win("SetupDiGetDeviceRegistryPropertyW", error));
            }
            match self.property_raw(property, required as usize) {
                Ok(raw) => return Ok(raw),
                Err(error)
                    if error.code == HRESULT::from_win32(ERROR_INSUFFICIENT_BUFFER.0).0 as u32 =>
                {
                    // The property grew after the size query; retry with a fresh size.
                    continue;
                }
                Err(error) => return Err(error),
            }
        }
        Err(Error::msg(
            "SetupDiGetDeviceRegistryPropertyW",
            "property kept growing during three reads",
        ))
    }
    /// Performs one property read with exactly the caller's buffer capacity, without retry.
    pub fn property_raw(
        &self,
        property: SETUP_DI_REGISTRY_PROPERTY,
        capacity: usize,
    ) -> Result<RawProperty> {
        if u32::try_from(capacity).is_err() {
            return Err(Error::msg(
                "SetupDiGetDeviceRegistryPropertyW",
                "capacity exceeds the SDK's u32 count",
            ));
        }
        let mut bytes = Vec::new();
        bytes
            .try_reserve_exact(capacity)
            .map_err(|e| Error::msg("SetupDiGetDeviceRegistryPropertyW", e.to_string()))?;
        bytes.resize(capacity, 0);
        let mut kind = 0;
        let mut required_size = 0;
        // C# parity: app/src/Services/DeviceCleaningService.cs:59-65
        // This one-call boundary deliberately does not resize a legacy 1024-byte read.
        // SAFETY: The device retains the live set and bytes has exactly capacity (checked as u32).
        unsafe {
            SetupDiGetDeviceRegistryPropertyW(
                self.set.handle,
                &self.data,
                property,
                Some(&mut kind),
                Some(&mut bytes),
                Some(&mut required_size),
            )
        }
        .map_err(|e| {
            let mut error = Error::from_win("SetupDiGetDeviceRegistryPropertyW", e);
            error
                .detail
                .push_str(&format!(" (capacity {capacity}, required {required_size})"));
            error
        })?;
        if required_size as usize > bytes.len() {
            return Err(Error::msg(
                "SetupDiGetDeviceRegistryPropertyW",
                "invalid output size",
            ));
        }
        bytes.truncate(required_size as usize);
        Ok(RawProperty {
            kind,
            required_size,
            bytes,
        })
    }
    /// Borrows the owning snapshot's handle for removal with this device data.
    pub fn set_handle(&self) -> HDEVINFO {
        self.set.as_raw()
    }
    /// Borrows the SDK device data; callers may copy it while retaining the snapshot.
    pub fn data(&self) -> &SP_DEVINFO_DATA {
        &self.data
    }
}

/// Maps uppercase instance IDs to their first hardware ID, for case-insensitive lookup.
pub fn hardware_id_map() -> Result<HashMap<String, String>> {
    let set = DevInfoSet::enum_present_all()?;
    let mut map = HashMap::new();
    for device in set.devices()? {
        // C# parity: app/src/Services/Win32/SetupApi.cs:94-105
        // Keep other devices' results on a per-device failure, but record the failure.
        let entry = (|| -> Result<(String, String)> {
            let instance_id = device.instance_id()?;
            // C# parity: app/src/Services/Win32/SetupApi.cs:101-108
            let raw = device.property_raw(SPDRP_HARDWAREID, 2048)?;
            let hardware_id = wide::from_wide(&property_units(&raw.bytes)?);
            Ok((instance_id.to_uppercase(), hardware_id))
        })();
        match entry {
            Ok((instance_id, hardware_id)) => {
                // C# parity: app/src/Services/Win32/SetupApi.cs:73-111
                if !hardware_id.is_empty() {
                    map.insert(instance_id, hardware_id);
                }
            }
            Err(error) => eprintln!("{error}"),
        }
    }
    Ok(map)
}
/// Returns uppercase present instance IDs, for case-insensitive membership checks.
pub fn present_instance_ids() -> Result<HashSet<String>> {
    let set = DevInfoSet::enum_present_all()?;
    set.devices()?
        .iter()
        .map(|device| device.instance_id().map(|id| id.to_uppercase()))
        .collect()
}

/// Rechecks an instance ID with normal-mode CM_Locate_DevNodeW; lookup errors stay errors.
pub fn is_present(instance_id: &str) -> Result<bool> {
    if instance_id.is_empty() || instance_id.contains('\0') {
        return Err(Error::msg(
            "CM_Locate_DevNodeW",
            "empty or NUL-containing instance ID",
        ));
    }
    let instance_id = wide::to_wide(instance_id);
    let mut devinst = 0;
    // SAFETY: The ID is NUL-terminated, devinst is writable, and NORMAL excludes phantom nodes.
    let status = unsafe {
        CM_Locate_DevNodeW(
            &mut devinst,
            PCWSTR(instance_id.as_ptr()),
            CM_LOCATE_DEVNODE_NORMAL,
        )
    };
    match status {
        CR_SUCCESS => Ok(true),
        CR_NO_SUCH_DEVNODE => Ok(false),
        _ => {
            // SAFETY: This pure status conversion accepts the SDK CONFIGRET and fallback code.
            let code = unsafe { CM_MapCrToWin32Err(status, ERROR_GEN_FAILURE.0) };
            let mut error = Error::from_win(
                "CM_Locate_DevNodeW",
                windows::core::Error::from_hresult(HRESULT::from_win32(code)),
            );
            error
                .detail
                .push_str(&format!(" (CONFIGRET 0x{:08X})", status.0));
            Err(error)
        }
    }
}

fn property_units(bytes: &[u8]) -> Result<Vec<u16>> {
    if !bytes.len().is_multiple_of(2) {
        return Err(Error::msg("SetupAPI UTF-16 property", "odd byte count"));
    }
    Ok(bytes
        .as_chunks::<2>()
        .0
        .iter()
        .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
        .collect())
}
