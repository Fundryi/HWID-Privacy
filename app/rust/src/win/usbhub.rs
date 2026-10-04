//! Read-only USB string descriptors, associated by the hub port's driver key.

use super::{Error, OwnedHandle, Result, catch_panic, ioctl, record};
use std::{
    collections::{HashMap, HashSet, VecDeque},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU32, Ordering},
        mpsc,
    },
    time::{Duration, Instant},
};
use windows::{
    Win32::Devices::DeviceAndDriverInstallation::{
        CM_GET_DEVICE_INTERFACE_LIST_PRESENT, CM_Get_Device_Interface_List_SizeW,
        CM_Get_Device_Interface_ListW, CR_BUFFER_SMALL, CR_SUCCESS,
    },
    core::{GUID, PCWSTR},
};

// usbioctl.h: USB structures are packed at byte alignment, including EX status at 31.
const HUB: GUID = GUID::from_u128(0xf18a0e88_c30c_11d0_8815_00a0c906bed8);
const NODE_INFO: u32 = 0x220408;
const CONNECTION_EX: u32 = 0x220448;
const DESCRIPTOR: u32 = 0x220410;
const DRIVER_KEY: u32 = 0x220420;
const BUDGET: Duration = Duration::from_millis(750);
const HUB_JOBS: usize = 2;
const SCAN_JOBS: usize = 4;
static BUSY: AtomicBool = AtomicBool::new(false);

struct Worker;
impl Drop for Worker {
    fn drop(&mut self) {
        BUSY.store(false, Ordering::Release);
    }
}

#[derive(Default)]
pub struct DeviceStrings {
    pub serial: Option<String>,
    pub manufacturer: Option<String>,
    pub product: Option<String>,
}

enum StringField {
    Serial,
    Manufacturer,
    Product,
}

enum ScanValue {
    Port(String),
    String(String, StringField, String),
}

/// Reads device strings keyed by SPDRP_DRIVER, waiting at most 750 ms for the entire scan.
pub fn descriptors() -> HashMap<String, DeviceStrings> {
    let mut serials = HashMap::new();
    if BUSY
        .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
        .is_err()
    {
        record(Error::msg(
            "USB hub serial",
            "previous hub query still running",
        ));
        return serials;
    }
    let deadline = Instant::now() + BUDGET;
    let (tx, rx) = mpsc::channel();
    // Synchronous driver calls cannot safely be forcibly terminated. The worker owns
    // every buffer/handle until the call returns. BUSY covers the coordinator AND
    // its scoped lanes, so later scans cannot accumulate stuck driver workers.
    let worker = Worker;
    let spawned = std::thread::Builder::new()
        .name("usb-serial".into())
        .spawn(move || {
            let _worker = worker;
            let result = catch_panic(|| scan(deadline, &tx));
            let error = match result {
                Ok(Ok(())) => return,
                Ok(Err(error)) => error,
                Err(_) => Error::msg("USB hub serial", "worker panicked"),
            };
            // Receiver disappearance means the caller already recorded its timeout.
            let _ = tx.send(Err(error));
        });
    if let Err(error) = spawned {
        record(Error::msg("USB hub worker", error.to_string()));
        return serials;
    }
    let mut ambiguous = HashSet::new();
    loop {
        match rx.recv_timeout(deadline.saturating_duration_since(Instant::now())) {
            Ok(Ok(ScanValue::Port(key))) => {
                if serials.contains_key(&key) || ambiguous.contains(&key) {
                    serials.remove(&key);
                    ambiguous.insert(key);
                    record(Error::msg(
                        "USB hub association",
                        "driver key belongs to multiple ports",
                    ));
                } else {
                    serials.insert(key, DeviceStrings::default());
                }
            }
            Ok(Ok(ScanValue::String(key, field, value))) => {
                if let Some(strings) = serials.get_mut(&key) {
                    let target = match field {
                        StringField::Serial => &mut strings.serial,
                        StringField::Manufacturer => &mut strings.manufacturer,
                        StringField::Product => &mut strings.product,
                    };
                    *target = Some(value);
                }
            }
            Ok(Err(error)) => record(error),
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
            Err(mpsc::RecvTimeoutError::Timeout) => {
                record(Error::msg(
                    "USB hub serial",
                    "750 ms scan deadline exceeded",
                ));
                break;
            }
        }
    }
    serials
}

fn hubs() -> Result<Vec<String>> {
    for _ in 0..3 {
        let mut count = 0;
        // SAFETY: GUID and count are live; null device ID requests all present hubs.
        let status = unsafe {
            CM_Get_Device_Interface_List_SizeW(
                &mut count,
                &HUB,
                PCWSTR::null(),
                CM_GET_DEVICE_INTERFACE_LIST_PRESENT,
            )
        };
        if status != CR_SUCCESS || !(2..=65536).contains(&count) {
            return Err(Error::msg(
                "USB hub interfaces",
                format!("size status {}, units {count}", status.0),
            ));
        }
        let mut buffer = vec![0; count as usize];
        // SAFETY: buffer has the queried UTF-16 capacity; no device filter is supplied.
        let status = unsafe {
            CM_Get_Device_Interface_ListW(
                &HUB,
                PCWSTR::null(),
                &mut buffer,
                CM_GET_DEVICE_INTERFACE_LIST_PRESENT,
            )
        };
        if status == CR_BUFFER_SMALL {
            continue;
        }
        if status != CR_SUCCESS {
            return Err(Error::msg(
                "USB hub interfaces",
                format!("list status {}", status.0),
            ));
        }
        return buffer
            .split(|unit| *unit == 0)
            .take_while(|s| !s.is_empty())
            .map(|s| String::from_utf16(s).map_err(|e| Error::msg("USB hub path", e.to_string())))
            .collect();
    }
    Err(Error::msg("USB hub interfaces", "list kept growing"))
}

fn query(
    hub: &OwnedHandle,
    deadline: Instant,
    code: u32,
    input: &[u8],
    capacity: usize,
) -> Result<Vec<u8>> {
    if Instant::now() >= deadline {
        return Err(Error::msg("USB hub serial", "scan deadline exceeded"));
    }
    ioctl::device_io_control(hub, code, input, capacity)
}

fn scan(deadline: Instant, tx: &mpsc::Sender<Result<ScanValue>>) -> Result<()> {
    let mut jobs = VecDeque::new();
    for path in hubs()? {
        let port = Arc::new(AtomicU32::new(1));
        for _ in 0..HUB_JOBS {
            jobs.push_back((path.clone(), Arc::clone(&port)));
        }
    }
    let jobs = Mutex::new(jobs);
    std::thread::scope(|scope| {
        for _ in 0..SCAN_JOBS {
            let jobs = &jobs;
            scope.spawn(move || {
                loop {
                    if Instant::now() >= deadline {
                        break;
                    }
                    let Some((path, next_port)) =
                        jobs.lock().unwrap_or_else(|e| e.into_inner()).pop_front()
                    else {
                        break;
                    };
                    let result = catch_panic(|| scan_hub(&path, &next_port, deadline, tx));
                    let error = match result {
                        Ok(Ok(())) => continue,
                        Ok(Err(error)) => error,
                        Err(_) => Error::msg("USB hub serial", "worker panicked"),
                    };
                    if tx.send(Err(error)).is_err() {
                        break;
                    }
                }
            });
        }
    });
    Ok(())
}

fn scan_hub(
    path: &str,
    next_port: &AtomicU32,
    deadline: Instant,
    tx: &mpsc::Sender<Result<ScanValue>>,
) -> Result<()> {
    // USBView uses GENERIC_WRITE for read-only hub IOCTL requests.
    // Each lane opens and owns its own handle; no handle crosses threads.
    let hub = ioctl::open_device(path, 0x40000000)?;
    let info = query(&hub, deadline, NODE_INFO, &[0; 76], 76)?;
    let ports = *info
        .get(6)
        .ok_or_else(|| Error::msg("USB hub info", "short descriptor"))?;
    loop {
        if Instant::now() >= deadline {
            break;
        }
        let port = next_port.fetch_add(1, Ordering::Relaxed);
        if port > u32::from(ports) {
            break;
        }
        if let Err(error) = port_strings(&hub, port, deadline, tx)
            && tx.send(Err(error)).is_err()
        {
            return Ok(());
        }
    }
    Ok(())
}

fn port_strings(
    hub: &OwnedHandle,
    port: u32,
    deadline: Instant,
    tx: &mpsc::Sender<Result<ScanValue>>,
) -> Result<()> {
    let mut request = [0; 4096];
    request[..4].copy_from_slice(&port.to_le_bytes());
    let info = query(hub, deadline, CONNECTION_EX, &request, request.len())?;
    if info.len() < 35 {
        return Err(Error::msg("USB connection", "short EX response"));
    }
    if info[31..35] != 1_u32.to_le_bytes() {
        return Ok(());
    }
    if info[4] != 18 || info[5] != 1 {
        return Err(Error::msg("USB connection", "invalid device descriptor"));
    }
    if info[18..21] == [0, 0, 0] {
        return Ok(());
    }
    let key = driver_key(hub, port, deadline)?;
    // Declare the port once. Later strings from this port update its entry;
    // a second port declaring the same key removes all its results as ambiguous.
    if tx.send(Ok(ScanValue::Port(key.clone()))).is_err() {
        return Ok(());
    }
    let languages = descriptor(hub, port, 0, 0, deadline)?;
    let language = *languages
        .first()
        .ok_or_else(|| Error::msg("USB LANGID", "empty language list"))?;
    // Serial first preserves the existing identity priority under the shared deadline.
    // Each independently successful string is rechecked before retention, so an
    // optional name read (including a timeout) cannot discard an already verified serial.
    for (index, field, operation) in [
        (info[20], StringField::Serial, "USB serial"),
        (info[18], StringField::Manufacturer, "USB manufacturer"),
        (info[19], StringField::Product, "USB product"),
    ] {
        if index == 0 {
            continue;
        }
        let result = (|| {
            let units = descriptor(hub, port, index, language, deadline)?;
            let value = String::from_utf16(&units)
                .map_err(|_| Error::msg(operation, "invalid UTF-16 string"))?;
            if value.chars().any(char::is_control)
                || (operation == "USB serial" && value.is_empty())
            {
                return Err(Error::msg(operation, "empty or control-containing string"));
            }
            recheck_port(hub, port, deadline, &request, &info, &key)?;
            Ok(value)
        })();
        match result {
            Ok(value) if !value.is_empty() => {
                // Publish immediately: a later synchronous request may outlive the
                // caller's wait cap, but cannot withhold this verified partial result.
                if tx
                    .send(Ok(ScanValue::String(key.clone(), field, value)))
                    .is_err()
                {
                    return Ok(());
                }
            }
            Ok(_) => {}
            Err(error) => record(error),
        }
    }
    Ok(())
}

fn recheck_port(
    hub: &OwnedHandle,
    port: u32,
    deadline: Instant,
    request: &[u8],
    info: &[u8],
    key: &str,
) -> Result<()> {
    // Port identity must survive each string request; do not associate by VID/PID.
    let after = query(hub, deadline, CONNECTION_EX, request, request.len())?;
    if after.get(4..22) != info.get(4..22)
        || after.get(31..35) != Some(&1_u32.to_le_bytes())
        || driver_key(hub, port, deadline)? != key
    {
        return Err(Error::msg(
            "USB hub association",
            "port changed during descriptor read",
        ));
    }
    Ok(())
}

fn driver_key(hub: &OwnedHandle, port: u32, deadline: Instant) -> Result<String> {
    let mut request = [0; 12];
    request[..4].copy_from_slice(&port.to_le_bytes());
    let bytes = query(hub, deadline, DRIVER_KEY, &request, 4096)?;
    let length = bytes
        .get(4..8)
        .ok_or_else(|| Error::msg("USB driver key", "short response"))?;
    let length = u32::from_le_bytes([length[0], length[1], length[2], length[3]]) as usize;
    let payload = bytes
        .get(8..length)
        .filter(|p| p.len() >= 4 && p.len().is_multiple_of(2))
        .ok_or_else(|| Error::msg("USB driver key", "invalid length"))?;
    let units: Vec<_> = payload
        .as_chunks::<2>()
        .0
        .iter()
        .map(|p| u16::from_le_bytes([p[0], p[1]]))
        .collect();
    let end = units
        .iter()
        .position(|v| *v == 0)
        .filter(|end| *end > 0)
        .ok_or_else(|| Error::msg("USB driver key", "missing name or terminator"))?;
    String::from_utf16(&units[..end])
        .map(|key| key.to_ascii_uppercase())
        .map_err(|e| Error::msg("USB driver key", e.to_string()))
}

fn descriptor(
    hub: &OwnedHandle,
    port: u32,
    index: u8,
    language: u16,
    deadline: Instant,
) -> Result<Vec<u16>> {
    let mut request = [0; 12];
    request[..4].copy_from_slice(&port.to_le_bytes());
    request[4..8].copy_from_slice(&[0x80, 6, index, 3]);
    request[8..10].copy_from_slice(&language.to_le_bytes());
    request[10..12].copy_from_slice(&255_u16.to_le_bytes());
    let bytes = query(hub, deadline, DESCRIPTOR, &request, 267)?;
    string_units(
        bytes
            .get(12..)
            .ok_or_else(|| Error::msg("USB string descriptor", "missing header"))?,
    )
}

fn string_units(bytes: &[u8]) -> Result<Vec<u16>> {
    let length = usize::from(
        *bytes
            .first()
            .ok_or_else(|| Error::msg("USB string descriptor", "empty response"))?,
    );
    if length < 2 || !length.is_multiple_of(2) || bytes.get(1) != Some(&3) || length > bytes.len() {
        return Err(Error::msg(
            "USB string descriptor",
            "invalid length or type",
        ));
    }
    Ok(bytes[2..length]
        .as_chunks::<2>()
        .0
        .iter()
        .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn usb_string_wire_fixtures() {
        assert_eq!(string_units(&[4, 3, 9, 4]).expect("LANGID"), [0x0409]);
        let units =
            string_units(&[6, 3, 65, 0, 233, 0, 0, 0]).expect("serial with trailing buffer");
        assert_eq!(String::from_utf16(&units).expect("UTF-16"), "Aé");
        for bad in [
            &[][..],
            &[0, 3],
            &[1, 3],
            &[3, 3, 65],
            &[4, 1, 65, 0],
            &[6, 3, 65, 0],
        ] {
            assert!(string_units(bad).is_err());
        }
        let units = string_units(&[4, 3, 0, 216]).expect("wire framing is valid");
        assert!(String::from_utf16(&units).is_err());
    }
}
