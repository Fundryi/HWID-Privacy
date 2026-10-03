//! SetupAPI snapshots retain ownership while borrowed devices are inspected.

use super::{Error, Result};
use std::collections::{HashMap, HashSet};
use windows::Win32::Devices::DeviceAndDriverInstallation::{
    HDEVINFO, SETUP_DI_REGISTRY_PROPERTY, SP_DEVINFO_DATA,
};

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
        Err(Error::msg("SetupAPI present snapshot", "not ported yet"))
    }
    /// Owns a snapshot containing present and historical devices of every class.
    pub fn enum_all() -> Result<Self> {
        Err(Error::msg("SetupAPI snapshot", "not ported yet"))
    }
    /// Enumerates devices borrowing this snapshot so the HDEVINFO stays alive.
    pub fn devices(&self) -> Result<Vec<Device<'_>>> {
        Err(Error::msg("SetupAPI enumeration", "not ported yet"))
    }
    /// Borrows the native set handle for guarded device removal.
    pub fn as_raw(&self) -> HDEVINFO {
        self.handle
    }

    /// Removes copied device data belonging to this live snapshot after the caller's guard.
    pub fn remove_device(&self, _data: &SP_DEVINFO_DATA) -> Result<()> {
        Err(Error::msg("SetupDiRemoveDevice", "not ported yet"))
    }
}

impl Drop for DevInfoSet {
    fn drop(&mut self) {
        // Stub constructors always return Err and never acquire a native set.
    }
}

impl Device<'_> {
    /// Reads the exact device instance ID from this snapshot.
    pub fn instance_id(&self) -> Result<String> {
        Err(Error::msg("SetupAPI instance ID", "not ported yet"))
    }
    /// Reads a UTF-16 registry property as a single string.
    pub fn property_string(&self, _property: SETUP_DI_REGISTRY_PROPERTY) -> Result<String> {
        Err(Error::msg("SetupAPI string property", "not ported yet"))
    }
    /// Reads a REG_MULTI_SZ registry property without joining its elements.
    pub fn property_multi_sz(&self, _property: SETUP_DI_REGISTRY_PROPERTY) -> Result<Vec<String>> {
        Err(Error::msg("SetupAPI multi-sz property", "not ported yet"))
    }
    /// Performs one property read with exactly the caller's buffer capacity, without retry.
    pub fn property_raw(
        &self,
        _property: SETUP_DI_REGISTRY_PROPERTY,
        _capacity: usize,
    ) -> Result<RawProperty> {
        Err(Error::msg("SetupAPI raw property", "not ported yet"))
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
    Err(Error::msg("SetupAPI hardware ID map", "not ported yet"))
}
/// Returns uppercase present instance IDs, for case-insensitive membership checks.
pub fn present_instance_ids() -> Result<HashSet<String>> {
    Err(Error::msg("SetupAPI present IDs", "not ported yet"))
}

/// Rechecks an instance ID with normal-mode CM_Locate_DevNodeW; lookup errors stay errors.
pub fn is_present(_instance_id: &str) -> Result<bool> {
    Err(Error::msg("CM_Locate_DevNodeW", "not ported yet"))
}
