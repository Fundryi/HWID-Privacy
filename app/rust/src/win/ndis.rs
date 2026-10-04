//! Best-effort, read-only legacy NDIS permanent-address corroboration.

use super::{Error, Result, catch_panic, ioctl, record};
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

struct Worker<'a>(&'a AtomicBool);
impl<'a> Worker<'a> {
    fn acquire(busy: &'a AtomicBool) -> Option<Self> {
        busy.compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .ok()
            .map(|_| Self(busy))
    }
}
impl Drop for Worker<'_> {
    fn drop(&mut self) {
        self.0.store(false, Ordering::Release);
    }
}

fn diagnostic(mut error: Error) -> Error {
    if error.code != 0 {
        error.detail = match error.code {
            5 => "access-denied: NDIS request denied",
            2 | 3 | 1168 => "absent: NDIS interface or property missing",
            258 | 1460 => "timeout: NDIS request timed out",
            _ => "unsupported: NDIS request failed",
        }
        .into();
    } else if !error.detail.contains(':') {
        let class = if error.detail.contains("deadline") || error.detail.contains("still running") {
            "timeout"
        } else {
            "malformed"
        };
        error.detail = format!("{class}: {}", error.detail);
    }
    error
}

/// Queries only accepted WMI instances; retains completed results within a 750 ms wait.
pub fn permanent_macs(instances: Vec<String>) -> PermanentMacs {
    let mut result = PermanentMacs::default();
    let Some(worker) = Worker::acquire(&BUSY) else {
        result.failures.push(Error::msg(
            "NDIS OID",
            "timeout: previous scan still running",
        ));
        return result;
    };
    let deadline = Instant::now() + BUDGET;
    let (tx, rx) = mpsc::channel();
    // Driver calls are synchronous. The worker keeps its buffers and handles alive
    // after timeout; BUSY prevents repeated collections from piling up stuck workers.
    let spawned = std::thread::Builder::new()
        .name("ndis-oid".into())
        .spawn(move || {
            let _worker = worker;
            let error = match catch_panic(|| {
                let targets = instances
                    .into_iter()
                    .map(|id| id.to_ascii_uppercase())
                    .collect();
                scan(&targets, deadline, &tx)
            }) {
                Ok(Ok(())) => return,
                Ok(Err(error)) => error,
                Err(_) => Error::msg("NDIS OID", "worker panicked"),
            };
            let _ = tx.send(Err(error));
        });
    if let Err(error) = spawned {
        result.failures.push(Error::msg(
            "NDIS OID worker",
            format!("unsupported: cannot spawn worker ({error})"),
        ));
        return result;
    }
    let mut seen = HashSet::new();
    loop {
        match rx.recv_timeout(deadline.saturating_duration_since(Instant::now())) {
            Ok(Ok((instance, address))) => {
                retain_address(&mut result, &mut seen, instance, address);
            }
            Ok(Err(error)) => result.failures.push(diagnostic(error)),
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
            Err(mpsc::RecvTimeoutError::Timeout) => {
                result
                    .failures
                    .push(Error::msg("NDIS OID", "750 ms scan deadline exceeded"));
                break;
            }
        }
    }
    reject_shared_addresses(&mut result);
    result
}

fn retain_address(
    result: &mut PermanentMacs,
    seen: &mut HashSet<String>,
    instance: String,
    address: Result<String>,
) {
    if !seen.insert(instance.clone()) {
        result.addresses.remove(&instance);
        result.failures.push(Error::msg(
            "NDIS OID association",
            "ambiguous: multiple interfaces match one instance",
        ));
    } else if let Ok(address) = &address {
        result.addresses.insert(instance, address.clone());
    }
    if let Err(error) = address {
        result.failures.push(diagnostic(error));
    }
}

fn reject_shared_addresses(result: &mut PermanentMacs) {
    let mut counts = HashMap::new();
    for address in result.addresses.values() {
        *counts.entry(address.clone()).or_insert(0_usize) += 1;
    }
    result.addresses.retain(|_, address| {
        if counts.get(address).copied().unwrap_or_default() > 1 {
            result.failures.push(Error::msg(
                "NDIS OID address",
                "implausible: permanent address shared by different adapters",
            ));
            false
        } else {
            true
        }
    });
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
    let mut matched: HashMap<String, Vec<String>> = HashMap::new();
    let mut complete = false;
    let mut malformed = false;
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
                complete = true;
                break;
            }
            Err(error) => return Err(Error::from_win("NDIS SetupDiEnumDeviceInterfaces", error)),
        }
        let (instance, path) = match interface_details(set.0, &interface) {
            Ok(details) => details,
            Err(error) => {
                malformed = true;
                let _ = tx.send(Err(error));
                continue;
            }
        };
        let instance = instance.to_ascii_uppercase();
        if !targets.contains(&instance) {
            continue;
        }
        matched.entry(instance).or_default().push(path);
    }
    if !complete || malformed {
        return Err(Error::msg(
            "NDIS OID association",
            "ambiguous: interface inventory incomplete or malformed",
        ));
    }
    // Prove uniqueness before a synchronous query can stall. A timeout during
    // enumeration must not retain an address whose second interface is unseen.
    let mut targets: Vec<_> = targets.iter().collect();
    targets.sort();
    for instance in targets {
        let address = match matched.get(instance).map(Vec::as_slice) {
            Some([path]) => {
                if Instant::now() >= deadline {
                    return Err(Error::msg(
                        "NDIS OID",
                        "timeout: 750 ms scan deadline exceeded",
                    ));
                }
                query_address(path)
            }
            Some(_) => Err(Error::msg(
                "NDIS OID association",
                "ambiguous: multiple interfaces match one instance",
            )),
            None => Err(Error::msg(
                "NDIS OID association",
                "absent: no present miniport interface matches WMI instance",
            )),
        };
        if tx.send(Ok((instance.clone(), address))).is_err() {
            return Ok(());
        }
    }
    Ok(())
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
    if required == 0 || instance.get(required.saturating_sub(1) as usize) != Some(&0) {
        return Err(Error::msg(
            "NDIS instance ID",
            "invalid instance ID size or terminator",
        ));
    }
    let instance = instance
        .get(..required as usize)
        .ok_or_else(|| Error::msg("NDIS instance ID", "malformed: instance ID exceeds buffer"))?;
    Ok((
        terminated_text(instance, "NDIS instance ID")?,
        terminated_text(path, "NDIS interface path")?,
    ))
}

fn terminated_text(units: &[u16], op: &'static str) -> Result<String> {
    let end = units
        .iter()
        .position(|unit| *unit == 0)
        .filter(|end| *end > 0)
        .ok_or_else(|| Error::msg(op, "malformed: empty or unterminated text"))?;
    let text = String::from_utf16(
        units
            .get(..end)
            .ok_or_else(|| Error::msg(op, "malformed: invalid text span"))?,
    )
    .map_err(|_| Error::msg(op, "malformed: invalid UTF-16"))?;
    if text.chars().any(char::is_control) {
        return Err(Error::msg(op, "malformed: control-containing text"));
    }
    Ok(text)
}

fn query_address(path: &str) -> Result<String> {
    let handle = ioctl::open_device(path, GENERIC_READ.0)?;
    let bytes = ioctl::device_io_control(
        &handle,
        IOCTL_NDIS_QUERY_GLOBAL_STATS,
        &OID_802_3_PERMANENT_ADDRESS.to_le_bytes(),
        32,
    )?;
    parse_address(&bytes)
}

fn parse_address(bytes: &[u8]) -> Result<String> {
    if bytes.len() != 6 {
        return Err(Error::msg(
            "OID_802_3_PERMANENT_ADDRESS",
            "malformed address: expected six bytes",
        ));
    }
    if bytes.iter().all(|byte| *byte == 0) || bytes.first().is_some_and(|byte| byte & 1 != 0) {
        return Err(Error::msg(
            "OID_802_3_PERMANENT_ADDRESS",
            "implausible: zero, broadcast or multicast address",
        ));
    }
    // Locally administered unicast addresses are valid driver reports.
    Ok(bytes
        .iter()
        .map(|byte| format!("{byte:02X}"))
        .collect::<Vec<_>>()
        .join(":"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ndis_busy_guard_survives_caller_timeout_and_resets_on_worker_exit() {
        static TEST_BUSY: AtomicBool = AtomicBool::new(false);
        let worker = Worker::acquire(&TEST_BUSY).expect("initial worker");
        let (release, blocked) = mpsc::channel();
        let (done, waited) = mpsc::channel();
        let thread = std::thread::spawn(move || {
            let _worker = worker;
            blocked.recv().expect("release");
            done.send(()).expect("done");
        });
        assert!(waited.recv_timeout(Duration::from_millis(1)).is_err());
        assert!(Worker::acquire(&TEST_BUSY).is_none());
        release.send(()).expect("release");
        thread.join().expect("worker");
        assert!(Worker::acquire(&TEST_BUSY).is_some());
    }

    #[test]
    fn oid_unicast_size_and_plausibility() {
        assert_eq!(
            parse_address(&[2, 0x7c, 0x39, 0x61, 0xb4, 0x8e]).expect("unicast"),
            "02:7C:39:61:B4:8E"
        );
        for bad in [
            &[][..],
            &[0; 5],
            &[0; 7],
            &[0; 6],
            &[255; 6],
            &[1, 2, 3, 4, 5, 6],
        ] {
            let mut result = PermanentMacs::default();
            retain_address(
                &mut result,
                &mut HashSet::new(),
                "PCI\\FABRICATED\\A".into(),
                parse_address(bad),
            );
            assert!(result.addresses.is_empty());
            assert_eq!(result.failures.len(), 1);
        }
    }

    #[test]
    fn oid_duplicate_interfaces_addresses_and_denial() {
        let mut result = PermanentMacs::default();
        let mut seen = HashSet::new();
        for instance in ["A", "A", "A"] {
            retain_address(
                &mut result,
                &mut seen,
                instance.into(),
                parse_address(&[2, 7, 9, 11, 13, 15]),
            );
        }
        assert!(result.addresses.is_empty());
        assert_eq!(result.failures.len(), 2);
        for instance in ["B", "C"] {
            retain_address(
                &mut result,
                &mut seen,
                instance.into(),
                parse_address(&[2, 17, 19, 21, 23, 25]),
            );
        }
        retain_address(
            &mut result,
            &mut seen,
            "D".into(),
            Err(Error {
                op: "NDIS OID",
                code: 5,
                detail: "access-denied".into(),
            }),
        );
        reject_shared_addresses(&mut result);
        assert!(result.addresses.is_empty());
        assert!(result.failures.iter().any(|e| e.code == 5));
        assert!(
            result
                .failures
                .iter()
                .any(|e| e.detail.contains("implausible"))
        );
    }

    #[test]
    fn ndis_exact_association_text_rejects_lossy_or_truncated_ids() {
        assert_eq!(
            terminated_text(
                &"PCI\\VEN_8086&DEV_2725\\A7C2\0"
                    .encode_utf16()
                    .collect::<Vec<_>>(),
                "NDIS instance ID"
            )
            .expect("instance"),
            "PCI\\VEN_8086&DEV_2725\\A7C2"
        );
        for bad in [&[][..], &[0], &[65], &[0xd800, 0], &[65, 10, 0]] {
            assert!(terminated_text(bad, "NDIS instance ID").is_err());
        }
    }
}
