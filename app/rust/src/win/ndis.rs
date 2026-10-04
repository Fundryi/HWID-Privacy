//! Best-effort, read-only legacy NDIS permanent-address corroboration.

use super::{Error, Result, catch_panic, ioctl, record, wide};
use std::{
    collections::{HashMap, HashSet},
    mem::{offset_of, size_of},
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    time::{Duration, Instant},
};
use windows::Win32::{
    Devices::DeviceAndDriverInstallation::{
        DIGCF_DEVICEINTERFACE, DIGCF_PRESENT, HDEVINFO, SP_DEVICE_INTERFACE_DATA,
        SP_DEVICE_INTERFACE_DETAIL_DATA_W, SP_DEVINFO_DATA, SetupDiDestroyDeviceInfoList,
        SetupDiEnumDeviceInterfaces, SetupDiGetClassDevsW, SetupDiGetDeviceInstanceIdW,
        SetupDiGetDeviceInterfaceDetailW,
    },
    Foundation::{ERROR_INSUFFICIENT_BUFFER, ERROR_NO_MORE_ITEMS, GENERIC_READ},
    NetworkManagement::Ndis::{GUID_NDIS_LAN_CLASS, OID_802_3_PERMANENT_ADDRESS},
};
use windows::core::{HRESULT, PCWSTR};

// ntddndis.h: CTL_CODE(FILE_DEVICE_PHYSICAL_NETCARD, 0, METHOD_OUT_DIRECT, FILE_ANY_ACCESS).
const IOCTL_NDIS_QUERY_GLOBAL_STATS: u32 = 0x00170002;
const BUDGET: Duration = Duration::from_millis(750);
static BUSY: AtomicBool = AtomicBool::new(false);

#[derive(Default)]
pub struct PermanentMacs {
    pub addresses: HashMap<String, String>,
    pub failures: Vec<Error>,
}

struct Worker;
impl Drop for Worker {
    fn drop(&mut self) {
        BUSY.store(false, Ordering::Release);
    }
}

/// Queries only accepted WMI instances; retains completed results within a 750 ms wait.
pub fn permanent_macs(instances: Vec<String>) -> PermanentMacs {
    let mut result = PermanentMacs::default();
    if BUSY
        .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
        .is_err()
    {
        result
            .failures
            .push(Error::msg("NDIS OID", "previous scan still running"));
        return result;
    }
    let deadline = Instant::now() + BUDGET;
    let (tx, rx) = mpsc::channel();
    let worker = Worker;
    // Driver calls are synchronous. The worker keeps its buffers and handles alive
    // after timeout; BUSY prevents repeated collections from piling up stuck workers.
    let spawned = std::thread::Builder::new()
        .name("ndis-oid".into())
        .spawn(move || {
            let _worker = worker;
            let targets = instances.into_iter().map(|id| id.to_uppercase()).collect();
            let error = match catch_panic(|| scan(&targets, deadline, &tx)) {
                Ok(Ok(())) => return,
                Ok(Err(error)) => error,
                Err(_) => Error::msg("NDIS OID", "worker panicked"),
            };
            let _ = tx.send(Err(error));
        });
    if let Err(error) = spawned {
        result
            .failures
            .push(Error::msg("NDIS OID worker", error.to_string()));
        return result;
    }
    let mut seen = HashSet::new();
    loop {
        match rx.recv_timeout(deadline.saturating_duration_since(Instant::now())) {
            Ok(Ok((instance, address))) => {
                let unique = seen.insert(instance.clone());
                if !unique {
                    result.addresses.remove(&instance);
                    result.failures.push(Error::msg(
                        "NDIS OID association",
                        "multiple interfaces match one instance",
                    ));
                }
                match address {
                    Ok(address) if unique => {
                        result.addresses.insert(instance, address);
                    }
                    Err(error) => result.failures.push(error),
                    Ok(_) => {}
                }
            }
            Ok(Err(error)) => result.failures.push(error),
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
            Err(mpsc::RecvTimeoutError::Timeout) => {
                result
                    .failures
                    .push(Error::msg("NDIS OID", "750 ms scan deadline exceeded"));
                break;
            }
        }
    }
    result
}

struct Interfaces(HDEVINFO);
impl Drop for Interfaces {
    fn drop(&mut self) {
        // SAFETY: This guard uniquely owns the set returned by SetupDiGetClassDevsW.
        if let Err(error) = unsafe { SetupDiDestroyDeviceInfoList(self.0) } {
            record(Error::from_win("NDIS SetupDiDestroyDeviceInfoList", error));
        }
    }
}

type Message = Result<(String, Result<String>)>;

fn scan(targets: &HashSet<String>, deadline: Instant, tx: &mpsc::Sender<Message>) -> Result<()> {
    // SAFETY: The interface GUID is live; no enumerator or parent is supplied.
    let set = Interfaces(
        unsafe {
            SetupDiGetClassDevsW(
                Some(&GUID_NDIS_LAN_CLASS),
                PCWSTR::null(),
                None,
                DIGCF_PRESENT | DIGCF_DEVICEINTERFACE,
            )
        }
        .map_err(|e| Error::from_win("NDIS SetupDiGetClassDevsW", e))?,
    );
    let mut matched = HashSet::new();
    for index in 0..1024 {
        if Instant::now() >= deadline {
            return Err(Error::msg("NDIS OID", "750 ms scan deadline exceeded"));
        }
        let mut interface = SP_DEVICE_INTERFACE_DATA {
            cbSize: size_of::<SP_DEVICE_INTERFACE_DATA>() as u32,
            ..Default::default()
        };
        // SAFETY: The set is live, the GUID matches its class, and the output has cbSize.
        match unsafe {
            SetupDiEnumDeviceInterfaces(set.0, None, &GUID_NDIS_LAN_CLASS, index, &mut interface)
        } {
            Ok(()) => {}
            Err(error) if error.code() == HRESULT::from_win32(ERROR_NO_MORE_ITEMS.0) => {
                for _ in targets.difference(&matched) {
                    let _ = tx.send(Err(Error::msg(
                        "NDIS OID association",
                        "no present miniport interface matches WMI instance",
                    )));
                }
                return Ok(());
            }
            Err(error) => return Err(Error::from_win("NDIS SetupDiEnumDeviceInterfaces", error)),
        }
        let (instance, path) = match interface_details(set.0, &interface) {
            Ok(details) => details,
            Err(error) => {
                let _ = tx.send(Err(error));
                continue;
            }
        };
        let instance = instance.to_uppercase();
        if !targets.contains(&instance) {
            continue;
        }
        matched.insert(instance.clone());
        let address = query_address(&path);
        if tx.send(Ok((instance, address))).is_err() {
            return Ok(());
        }
    }
    Err(Error::msg(
        "NDIS interface enumeration",
        "1024 interface limit exceeded",
    ))
}

fn interface_details(
    set: HDEVINFO,
    interface: &SP_DEVICE_INTERFACE_DATA,
) -> Result<(String, String)> {
    let mut required = 0;
    // SAFETY: The set/interface are live; no output buffer requests the required byte size.
    let query = unsafe {
        SetupDiGetDeviceInterfaceDetailW(set, interface, None, 0, Some(&mut required), None)
    };
    if let Err(error) = query
        && error.code() != HRESULT::from_win32(ERROR_INSUFFICIENT_BUFFER.0)
    {
        return Err(Error::from_win("NDIS interface detail size", error));
    }
    if !(size_of::<SP_DEVICE_INTERFACE_DETAIL_DATA_W>() as u32..=65536).contains(&required)
        || !required.is_multiple_of(2)
    {
        return Err(Error::msg(
            "NDIS interface detail",
            "invalid detail buffer size",
        ));
    }
    // u64 backing ensures alignment for the SDK structure, including its DWORD cbSize.
    let mut buffer = vec![0_u64; (required as usize).div_ceil(size_of::<u64>())];
    let detail = buffer
        .as_mut_ptr()
        .cast::<SP_DEVICE_INTERFACE_DETAIL_DATA_W>();
    let mut device = SP_DEVINFO_DATA {
        cbSize: size_of::<SP_DEVINFO_DATA>() as u32,
        ..Default::default()
    };
    let capacity = required;
    // SAFETY: detail is aligned, large enough, and live; device has the required cbSize.
    unsafe {
        (*detail).cbSize = size_of::<SP_DEVICE_INTERFACE_DETAIL_DATA_W>() as u32;
        SetupDiGetDeviceInterfaceDetailW(
            set,
            interface,
            Some(detail),
            capacity,
            Some(&mut required),
            Some(&mut device),
        )
    }
    .map_err(|e| Error::from_win("NDIS interface detail", e))?;
    let offset = offset_of!(SP_DEVICE_INTERFACE_DETAIL_DATA_W, DevicePath);
    if required > capacity || required as usize <= offset || !required.is_multiple_of(2) {
        return Err(Error::msg(
            "NDIS interface detail",
            "invalid returned detail size",
        ));
    }
    // SAFETY: The path offset is u16-aligned and the returned span fits the live allocation.
    let path = unsafe {
        std::slice::from_raw_parts(
            buffer.as_ptr().cast::<u8>().add(offset).cast::<u16>(),
            (required as usize - offset) / 2,
        )
    };
    if !path.contains(&0) || path.first() == Some(&0) {
        return Err(Error::msg(
            "NDIS interface detail",
            "empty or unterminated device path",
        ));
    }
    let mut instance = [0_u16; 512];
    // SAFETY: device came from this set; the writable instance slice has its stated length.
    unsafe { SetupDiGetDeviceInstanceIdW(set, &device, Some(&mut instance), Some(&mut required)) }
        .map_err(|e| Error::from_win("NDIS instance ID", e))?;
    if required == 0 || required as usize > instance.len() || instance[required as usize - 1] != 0 {
        return Err(Error::msg(
            "NDIS instance ID",
            "invalid instance ID size or terminator",
        ));
    }
    Ok((
        wide::from_wide(&instance[..required as usize]),
        wide::from_wide(path),
    ))
}

fn query_address(path: &str) -> Result<String> {
    let handle = ioctl::open_device(path, GENERIC_READ.0)?;
    let bytes = ioctl::device_io_control(
        &handle,
        IOCTL_NDIS_QUERY_GLOBAL_STATS,
        &OID_802_3_PERMANENT_ADDRESS.to_le_bytes(),
        32,
    )?;
    if bytes.len() != 6 {
        return Err(Error::msg(
            "OID_802_3_PERMANENT_ADDRESS",
            "malformed address: expected six bytes",
        ));
    }
    // Retain exactly the driver-reported bytes, including zero or locally administered values.
    Ok(bytes
        .iter()
        .map(|byte| format!("{byte:02X}"))
        .collect::<Vec<_>>()
        .join(":"))
}
