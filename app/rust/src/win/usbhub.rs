//! Read-only USB string descriptors, associated by the hub port's driver key.

use super::{Error, OwnedHandle, Result, catch_panic, ioctl};
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

fn record(mut error: Error) {
    if error.code != 0 {
        error.detail = match error.code {
            5 => "access-denied: USB request denied",
            2 | 3 | 1168 => "absent: USB device or property missing",
            258 | 1460 => "timeout: USB request timed out",
            _ => "unsupported: USB request failed",
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
    super::record(error);
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
    AssociationsReady,
    Port(String),
    String(String, StringField, String),
}

/// Reads device strings keyed by SPDRP_DRIVER, waiting at most 750 ms for the entire scan.
pub fn descriptors() -> HashMap<String, DeviceStrings> {
    let mut serials = HashMap::new();
    let Some(worker) = Worker::acquire(&BUSY) else {
        record(Error::msg(
            "USB hub serial",
            "previous hub query still running",
        ));
        return serials;
    };
    let deadline = Instant::now() + BUDGET;
    let (tx, rx) = mpsc::channel();
    // Synchronous driver calls cannot safely be forcibly terminated. The worker owns
    // every buffer/handle until the call returns. BUSY covers the coordinator AND
    // its scoped lanes, so later scans cannot accumulate stuck driver workers.
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
        record(Error::msg(
            "USB hub worker",
            format!("unsupported: cannot spawn worker ({error})"),
        ));
        return serials;
    }
    let mut ambiguous = HashSet::new();
    let mut associations_ready = false;
    loop {
        match rx.recv_timeout(deadline.saturating_duration_since(Instant::now())) {
            Ok(Ok(ScanValue::AssociationsReady)) => associations_ready = true,
            Ok(Ok(ScanValue::Port(key))) => {
                if let Err(error) = declare_port(&mut serials, &mut ambiguous, key) {
                    record(error);
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
                } else {
                    record(Error::msg(
                        "USB hub association",
                        "ambiguous: string belongs to an unresolved or duplicate driver key",
                    ));
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
    if !associations_ready {
        serials.clear();
        record(Error::msg(
            "USB hub association",
            "ambiguous: port inventory incomplete; descriptors omitted",
        ));
    }
    let mut counts = HashMap::new();
    for strings in serials.values() {
        if let Some(serial) = &strings.serial {
            *counts.entry(serial).or_insert(0_usize) += 1;
        }
    }
    if counts.values().any(|count| *count > 1) {
        // Cheap devices commonly reuse serials. These are independently proven
        // port/driver-key reports, never a serial-based join or uniqueness claim.
        record(Error::msg(
            "USB descriptor serial",
            "ambiguous: nonunique serials retained on independently matched ports",
        ));
    }
    serials
}

fn declare_port(
    strings: &mut HashMap<String, DeviceStrings>,
    ambiguous: &mut HashSet<String>,
    key: String,
) -> Result<()> {
    if strings.contains_key(&key) || ambiguous.contains(&key) {
        strings.remove(&key);
        ambiguous.insert(key);
        Err(Error::msg(
            "USB hub association",
            "ambiguous: driver key belongs to multiple ports",
        ))
    } else {
        strings.insert(key, DeviceStrings::default());
        Ok(())
    }
}

struct Port {
    path: String,
    number: u32,
    info: Vec<u8>,
    key: String,
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
    let ports = Mutex::new(Vec::new());
    let complete = AtomicBool::new(true);
    std::thread::scope(|scope| {
        for _ in 0..SCAN_JOBS {
            let jobs = &jobs;
            let ports = &ports;
            let complete = &complete;
            scope.spawn(move || {
                loop {
                    if Instant::now() >= deadline {
                        complete.store(false, Ordering::Relaxed);
                        break;
                    }
                    let Some((path, next_port)) =
                        jobs.lock().unwrap_or_else(|e| e.into_inner()).pop_front()
                    else {
                        break;
                    };
                    let result = catch_panic(|| scan_hub(&path, &next_port, deadline, ports));
                    let error = match result {
                        Ok(Ok(())) => continue,
                        Ok(Err(error)) => error,
                        Err(_) => Error::msg("USB hub serial", "worker panicked"),
                    };
                    complete.store(false, Ordering::Relaxed);
                    if tx.send(Err(error)).is_err() {
                        break;
                    }
                }
            });
        }
    });
    if !complete.load(Ordering::Relaxed) {
        return Err(Error::msg(
            "USB hub association",
            "ambiguous: port inventory incomplete",
        ));
    }
    let mut by_hub: HashMap<String, Vec<Port>> = HashMap::new();
    for port in ports.into_inner().unwrap_or_else(|e| e.into_inner()) {
        if tx.send(Ok(ScanValue::Port(port.key.clone()))).is_err() {
            return Ok(());
        }
        by_hub.entry(port.path.clone()).or_default().push(port);
    }
    if tx.send(Ok(ScanValue::AssociationsReady)).is_err() {
        return Ok(());
    }
    let mut jobs = VecDeque::new();
    for (path, ports) in by_hub {
        let ports = Arc::new(ports);
        let next = Arc::new(AtomicU32::new(0));
        for _ in 0..HUB_JOBS {
            jobs.push_back((path.clone(), Arc::clone(&ports), Arc::clone(&next)));
        }
    }
    let jobs = Mutex::new(jobs);
    std::thread::scope(|scope| {
        for _ in 0..SCAN_JOBS {
            let jobs = &jobs;
            scope.spawn(move || {
                let result = catch_panic(|| -> Result<()> {
                    while Instant::now() < deadline {
                        let Some((path, ports, next)) =
                            jobs.lock().unwrap_or_else(|e| e.into_inner()).pop_front()
                        else {
                            break;
                        };
                        let hub = ioctl::open_device(&path, 0x40000000)?;
                        while Instant::now() < deadline {
                            let index = next.fetch_add(1, Ordering::Relaxed) as usize;
                            let Some(port) = ports.get(index) else {
                                break;
                            };
                            if let Err(error) = port_strings(&hub, port, deadline, tx) {
                                let _ = tx.send(Err(error));
                            }
                        }
                    }
                    Ok(())
                });
                match result {
                    Ok(Ok(())) => {}
                    Ok(Err(error)) => {
                        let _ = tx.send(Err(error));
                    }
                    Err(_) => {
                        let _ = tx.send(Err(Error::msg(
                            "USB descriptors",
                            "malformed: worker panicked",
                        )));
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
    ports_found: &Mutex<Vec<Port>>,
) -> Result<()> {
    // USBView uses GENERIC_WRITE for read-only hub IOCTL requests.
    // Each lane opens and owns its own handle; no handle crosses threads.
    let hub = ioctl::open_device(path, 0x40000000)?;
    let info = query(&hub, deadline, NODE_INFO, &[0; 76], 76)?;
    let ports = *info
        .get(6)
        .ok_or_else(|| Error::msg("USB hub info", "short descriptor"))?;
    if info.get(..4) != Some(&0_u32.to_le_bytes())
        || !info.get(5).is_some_and(|kind| matches!(kind, 0x29 | 0x2a))
    {
        return Err(Error::msg(
            "USB hub info",
            "malformed: invalid hub descriptor",
        ));
    }
    loop {
        if Instant::now() >= deadline {
            return Err(Error::msg(
                "USB hub inventory",
                "timeout: scan deadline exceeded",
            ));
        }
        let port = next_port.fetch_add(1, Ordering::Relaxed);
        if port > u32::from(ports) {
            break;
        }
        let mut request = [0; 4096];
        request[..4].copy_from_slice(&port.to_le_bytes());
        let info = query(&hub, deadline, CONNECTION_EX, &request, request.len())?;
        if connected_indices(&info, port)?.is_none() {
            continue;
        }
        let key = driver_key(&hub, port, deadline)?;
        ports_found
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push(Port {
                path: path.into(),
                number: port,
                info,
                key,
            });
    }
    Ok(())
}

fn port_strings(
    hub: &OwnedHandle,
    port: &Port,
    deadline: Instant,
    tx: &mpsc::Sender<Result<ScanValue>>,
) -> Result<()> {
    let mut request = [0; 4096];
    request[..4].copy_from_slice(&port.number.to_le_bytes());
    let Some([manufacturer, product, serial]) = connected_indices(&port.info, port.number)? else {
        return Ok(());
    };
    if [manufacturer, product, serial] == [0; 3] {
        return Err(Error::msg(
            "USB descriptors",
            "absent: no string descriptor indices",
        ));
    }
    let languages = descriptor(hub, port.number, 0, 0, deadline)?;
    let language = language_id(&languages)?;
    // Serial first preserves the existing identity priority under the shared deadline.
    // Each independently successful string is rechecked before retention, so an
    // optional name read (including a timeout) cannot discard an already verified serial.
    for (index, field, operation) in [
        (serial, StringField::Serial, "USB serial"),
        (manufacturer, StringField::Manufacturer, "USB manufacturer"),
        (product, StringField::Product, "USB product"),
    ] {
        if index == 0 {
            record(Error::msg(operation, "absent: descriptor index is zero"));
            continue;
        }
        let result = (|| {
            let units = descriptor(hub, port.number, index, language, deadline)?;
            let value = descriptor_text(&units, operation)?;
            recheck_port(hub, port.number, deadline, &request, &port.info, &port.key)?;
            Ok(value)
        })();
        match result {
            Ok(value) if !value.is_empty() => {
                // Publish immediately: a later synchronous request may outlive the
                // caller's wait cap, but cannot withhold this verified partial result.
                if tx
                    .send(Ok(ScanValue::String(port.key.clone(), field, value)))
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
    verify_port(info, &after, port, key, &driver_key(hub, port, deadline)?)
}

fn connected_indices(info: &[u8], port: u32) -> Result<Option<[u8; 3]>> {
    if info.get(31..35).is_none() {
        return Err(Error::msg("USB connection", "malformed: short EX response"));
    }
    if info.get(31..35) == Some(&0_u32.to_le_bytes()) {
        // Disconnected ports are not failed optional identity reads.
        return Ok(None);
    }
    if info.get(31..35) != Some(&1_u32.to_le_bytes()) {
        return Err(Error::msg(
            "USB connection",
            "unsupported: connection status does not prove a connected device",
        ));
    }
    if info.get(..4) != Some(&port.to_le_bytes()) {
        return Err(Error::msg(
            "USB connection",
            "malformed: wrong port in connected EX response",
        ));
    }
    if info.get(4..6) != Some(&[18, 1]) {
        return Err(Error::msg(
            "USB connection",
            "malformed: invalid device descriptor",
        ));
    }
    let indices = info
        .get(18..21)
        .and_then(|bytes| bytes.try_into().ok())
        .ok_or_else(|| Error::msg("USB connection", "malformed: missing string indices"))?;
    Ok(Some(indices))
}

fn verify_port(before: &[u8], after: &[u8], port: u32, key: &str, after_key: &str) -> Result<()> {
    if connected_indices(after, port)?.is_none()
        || before.get(4..22).is_none()
        || after.get(4..22) != before.get(4..22)
        || after_key != key
    {
        return Err(Error::msg(
            "USB hub association",
            "ambiguous: port changed during descriptor read",
        ));
    }
    Ok(())
}

fn language_id(languages: &[u16]) -> Result<u16> {
    languages
        .first()
        .copied()
        .filter(|language| *language != 0 && *language != u16::MAX)
        .ok_or_else(|| Error::msg("USB LANGID", "malformed: empty or invalid first language"))
}

fn descriptor_text(units: &[u16], operation: &'static str) -> Result<String> {
    let text = String::from_utf16(units)
        .map_err(|_| Error::msg(operation, "malformed: invalid UTF-16 string"))?;
    if text.chars().any(char::is_control) {
        return Err(Error::msg(
            operation,
            "malformed: control-containing string",
        ));
    }
    let trimmed = text.trim();
    if trimmed.is_empty()
        || matches!(
            trimmed.to_ascii_lowercase().as_str(),
            "unknown" | "none" | "n/a" | "not available"
        )
    {
        return Err(Error::msg(
            operation,
            "placeholder: empty or unavailable descriptor string",
        ));
    }
    if operation == "USB serial"
        && trimmed.len() > 1
        && trimmed
            .chars()
            .next()
            .is_some_and(|first| trimmed.chars().all(|ch| ch == first))
    {
        return Err(Error::msg(
            operation,
            "implausible: repeated-character descriptor serial",
        ));
    }
    Ok(text)
}

fn driver_key(hub: &OwnedHandle, port: u32, deadline: Instant) -> Result<String> {
    let mut request = [0; 12];
    request[..4].copy_from_slice(&port.to_le_bytes());
    let bytes = query(hub, deadline, DRIVER_KEY, &request, 4096)?;
    parse_driver_key(&bytes, port)
}

fn parse_driver_key(bytes: &[u8], port: u32) -> Result<String> {
    if bytes.get(..4) != Some(&port.to_le_bytes()) {
        return Err(Error::msg("USB driver key", "malformed: wrong port"));
    }
    let length = bytes
        .get(4..8)
        .ok_or_else(|| Error::msg("USB driver key", "short response"))?;
    let length = u32::from_le_bytes(
        length
            .try_into()
            .map_err(|_| Error::msg("USB driver key", "malformed: invalid length field"))?,
    ) as usize;
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
    let units = units
        .get(..end)
        .ok_or_else(|| Error::msg("USB driver key", "malformed: invalid name span"))?;
    let text = String::from_utf16(units)
        .map_err(|_| Error::msg("USB driver key", "malformed: invalid UTF-16"))?;
    if text.chars().any(char::is_control) {
        return Err(Error::msg(
            "USB driver key",
            "malformed: control-containing name",
        ));
    }
    Ok(text.to_ascii_uppercase())
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
    Ok(bytes
        .get(2..length)
        .ok_or_else(|| Error::msg("USB string descriptor", "malformed: invalid payload span"))?
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
    fn usb_busy_guard_survives_caller_timeout_and_resets_on_worker_exit() {
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

    #[test]
    fn usb_descriptor_text_langid_and_hotplug_fail_closed() {
        assert_eq!(language_id(&[0x0409, 0x0407]).expect("language"), 0x0409);
        assert_eq!(
            descriptor_text(
                &"Acme Audio".encode_utf16().collect::<Vec<_>>(),
                "USB product"
            )
            .expect("product"),
            "Acme Audio"
        );
        for (units, op) in [
            (vec![0xd800], "USB product"),
            (vec![65, 10], "USB manufacturer"),
            ("000000".encode_utf16().collect(), "USB serial"),
            ("unknown".encode_utf16().collect(), "USB serial"),
        ] {
            let error = descriptor_text(&units, op).expect_err("bad string");
            let mut out = crate::report::Out::new();
            out.info("Device", "Acme USB").id("Serial", "R7Q291E4");
            out.fallback_failed(op, &error);
            let section = out.finish();
            assert_eq!(section.body, "Device: Acme USB\r\nSerial: R7Q291E4\r\n");
            assert_eq!(section.failures.len(), 1);
        }
        for langs in [&[][..], &[0], &[u16::MAX]] {
            assert!(language_id(langs).is_err());
        }
        let mut info = vec![0; 35];
        info[..4].copy_from_slice(&3_u32.to_le_bytes());
        info[4..6].copy_from_slice(&[18, 1]);
        info[18..21].copy_from_slice(&[1, 2, 3]);
        info[31..35].copy_from_slice(&1_u32.to_le_bytes());
        assert_eq!(
            connected_indices(&info, 3).expect("connection"),
            Some([1, 2, 3])
        );
        assert!(verify_port(&info, &info, 3, "KEY", "KEY").is_ok());
        assert!(verify_port(&info, &info, 3, "KEY", "OTHER").is_err());
        assert!(connected_indices(&info[..20], 3).is_err());
        assert!(connected_indices(&info, 4).is_err());
        let mut after = info.clone();
        after[12] = 1;
        assert!(verify_port(&info, &after, 3, "KEY", "KEY").is_err());
        after[31] = 0;
        assert!(verify_port(&info, &after, 3, "KEY", "KEY").is_err());
        after[..4].fill(0);
        assert_eq!(
            connected_indices(&after, 3).expect("disconnected port need not echo its index"),
            None
        );
        after[31] = 2;
        assert!(connected_indices(&after, 3).is_err());
    }

    #[test]
    fn usb_driver_key_requires_exact_port_encoding_and_size() {
        let mut wire = 7_u32.to_le_bytes().to_vec();
        let text: Vec<_> = "{36FC9E60-C465-11CF-8056-444553540000}\\0007\0"
            .encode_utf16()
            .flat_map(u16::to_le_bytes)
            .collect();
        wire.extend_from_slice(&((8 + text.len()) as u32).to_le_bytes());
        wire.extend(text);
        assert!(parse_driver_key(&wire, 7).is_ok());
        assert!(parse_driver_key(&wire, 8).is_err());
        assert!(parse_driver_key(&wire[..9], 7).is_err());
        wire[8..10].copy_from_slice(&0xd800_u16.to_le_bytes());
        assert!(parse_driver_key(&wire, 7).is_err());
    }

    #[test]
    fn usb_duplicate_port_keys_stay_ambiguous_after_third_port() {
        let mut strings = HashMap::new();
        let mut ambiguous = HashSet::new();
        declare_port(&mut strings, &mut ambiguous, "KEY".into()).expect("first port");
        assert!(strings.contains_key("KEY"));
        for _ in 0..2 {
            let error = declare_port(&mut strings, &mut ambiguous, "KEY".into())
                .expect_err("duplicate key");
            let mut out = crate::report::Out::new();
            out.fallback_failed("USB hub association", &error);
            assert_eq!(out.finish().failures.len(), 1);
            assert!(!strings.contains_key("KEY"));
        }
    }
}
