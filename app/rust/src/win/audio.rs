//! Read-only Core Audio endpoint inventory. COM objects never leave their worker.

use super::{Error, Result, catch_panic, record, wide};
use std::{
    marker::PhantomData,
    rc::Rc,
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    time::{Duration, Instant},
};
use windows::Win32::{
    Devices::FunctionDiscovery::{
        PKEY_Device_FriendlyName, PKEY_Device_InstanceId, PKEY_DeviceInterface_FriendlyName,
    },
    Foundation::{PROPERTYKEY, RPC_E_CHANGED_MODE},
    Media::Audio::{
        DEVICE_STATE, DEVICE_STATE_ACTIVE, DEVICE_STATE_DISABLED, DEVICE_STATE_NOTPRESENT,
        DEVICE_STATE_UNPLUGGED, IDeviceTopology, IMMDevice, IMMDeviceEnumerator,
        MMDeviceEnumerator, eCapture, eRender,
    },
    System::{
        Com::{
            CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED, COINIT_MULTITHREADED, CoCreateInstance,
            CoInitializeEx, CoTaskMemFree, CoUninitialize, STGM_READ,
            StructuredStorage::{PROPVARIANT, PropVariantClear},
        },
        Variant::{VT_CLSID, VT_EMPTY, VT_LPWSTR, VT_NULL},
    },
    UI::Shell::PropertiesSystem::IPropertyStore,
};
use windows::core::{GUID, PCWSTR, PWSTR};

// Microsoft.Windows.SDK.CPP 10.0.28000.2705, um/mmdeviceapi.h. This key is
// absent from windows 0.62.2; Windows 11 24H2+ may return VT_EMPTY even here.
const STABLE_ID: PROPERTYKEY = PROPERTYKEY {
    fmtid: GUID::from_u128(0x1da5d803_d492_4edd_8c23_e0c0ffee7f0e),
    pid: 12,
};

const BUDGET: Duration = Duration::from_secs(5);
const ENDPOINT_LIMIT: u32 = 4096;
static BUSY: AtomicBool = AtomicBool::new(false);

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

enum Message {
    Endpoint(usize, Endpoint),
    Failures(Vec<Error>),
    Finished(Result<Scan>),
}

#[derive(Clone, Debug)]
pub struct Endpoint {
    pub direction: &'static str,
    pub id: Option<String>,
    pub name: Option<String>,
    pub adapter: Option<String>,
    pub instance_id: Option<String>,
    pub stable_id: Option<String>,
}

#[derive(Default)]
pub struct Scan {
    pub endpoints: Vec<Endpoint>,
    pub failures: Vec<Error>,
    pub inactive: Vec<(&'static str, u32)>,
    pub incomplete: bool,
}

struct Apartment(PhantomData<Rc<()>>);

impl Apartment {
    fn new() -> Result<Self> {
        // SAFETY: Null reserved argument; this thread-bound guard balances success.
        let initialized = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };
        let initialized = if initialized == RPC_E_CHANGED_MODE {
            // SAFETY: Retain an existing STA, matching win/wmi.rs, and balance success.
            unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) }
        } else {
            initialized
        };
        initialized
            .ok()
            .map_err(|e| api_error("CoInitializeEx", e))?;
        Ok(Self(PhantomData))
    }
}

impl Drop for Apartment {
    fn drop(&mut self) {
        // SAFETY: One successful initialization on this thread; proxies drop first.
        unsafe { CoUninitialize() };
    }
}

struct Property(PROPVARIANT);

impl Drop for Property {
    fn drop(&mut self) {
        // SAFETY: GetValue transferred one initialized PROPVARIANT to this guard.
        if let Err(e) = unsafe { PropVariantClear(&mut self.0) } {
            record(api_error("PropVariantClear", e));
        }
    }
}

// Avoid IErrorInfo descriptions: a COM server could include an identity in them.
fn api_error(op: &'static str, error: windows::core::Error) -> Error {
    let code = error.code().0 as u32;
    let code = if code & 0xffff0000 == 0x80070000 {
        code & 0xffff
    } else {
        code
    };
    let detail = match code {
        5 => "access-denied: Core Audio request denied",
        1168 => "absent: property or endpoint absent",
        50 | 0x80004001 => "unsupported: Core Audio operation unsupported",
        1460 | 0x800705b4 => "timeout: Core Audio operation timed out",
        _ => "unsupported: Core Audio request failed",
    };
    Error {
        op,
        code,
        detail: detail.into(),
    }
}

// SAFETY contract: pointer comes from Core Audio and remains owned/alive while read.
unsafe fn string(pointer: PWSTR, op: &'static str) -> Result<String> {
    if pointer.is_null() {
        return Err(Error::msg(op, "malformed null string"));
    }
    // Core Audio promises a terminated UTF-16 string. Cap traversal/output rather
    // than accepting an arbitrarily large provider value; preserve all characters.
    for len in 0..32_768 {
        // SAFETY: The caller supplies a valid SDK-owned terminated string.
        if unsafe { *pointer.0.add(len) } == 0 {
            // SAFETY: Every unit through len has just been visited; the owner is alive.
            return validated_text(unsafe { std::slice::from_raw_parts(pointer.0, len) }, op);
        }
    }
    Err(Error::msg(op, "string exceeds 32767 UTF-16 units"))
}

fn validated_text(units: &[u16], op: &'static str) -> Result<String> {
    let text = String::from_utf16(units).map_err(|_| Error::msg(op, "malformed: UTF-16 string"))?;
    if text.chars().any(char::is_control) {
        return Err(Error::msg(op, "malformed: control-containing string"));
    }
    Ok(text)
}

fn stable_text(text: String, op: &'static str) -> Result<String> {
    let trimmed = text.trim();
    if trimmed.is_empty()
        || matches!(
            trimmed.to_ascii_lowercase().as_str(),
            "0" | "unknown" | "none" | "n/a" | "not available"
        )
    {
        return Err(Error::msg(
            op,
            "placeholder: empty or unavailable stable ID",
        ));
    }
    if trimmed.len() > 1
        && trimmed
            .chars()
            .next()
            .is_some_and(|first| trimmed.chars().all(|ch| ch == first))
    {
        return Err(Error::msg(op, "implausible: repeated-character stable ID"));
    }
    if let Ok(guid) = GUID::try_from(trimmed.trim_matches(['{', '}']))
        && (guid == GUID::zeroed() || guid == GUID::from_u128(u128::MAX))
    {
        return Err(Error::msg(op, "placeholder: null or all-FF stable GUID"));
    }
    Ok(text)
}

fn adapter_id(id: String) -> Result<String> {
    let mut parts = id.split('\\');
    let valid = (0..3).all(|_| parts.next().is_some_and(|part| !part.trim().is_empty()))
        && parts.next().is_none();
    let upper = id.to_ascii_uppercase();
    if !valid || id.chars().any(char::is_control) {
        return Err(Error::msg(
            "audio adapter association",
            "malformed: invalid devnode ID",
        ));
    }
    if upper.starts_with("SWD\\MMDEVAPI\\")
        || upper.starts_with("HTREE\\ROOT\\")
        || upper.starts_with("ROOT\\ROOT\\")
    {
        return Err(Error::msg(
            "audio adapter association",
            "ambiguous: endpoint or root devnode is not an adapter",
        ));
    }
    Ok(id)
}

fn property(store: &IPropertyStore, key: &PROPERTYKEY, op: &'static str) -> Result<Option<String>> {
    // SAFETY: The store is live on its apartment; key is a valid PROPERTYKEY.
    let value = Property(unsafe { store.GetValue(key) }.map_err(|e| api_error(op, e))?);
    // SAFETY: GetValue initialized the discriminant and its corresponding union.
    let raw = unsafe { &value.0.Anonymous.Anonymous };
    if raw.vt == VT_EMPTY || raw.vt == VT_NULL {
        return Ok(None);
    }
    if raw.vt == VT_CLSID && key == &STABLE_ID {
        // SAFETY: VT_CLSID selects puuid; the property guard retains its allocation.
        let pointer = unsafe { raw.Anonymous.puuid };
        if pointer.is_null() {
            return Err(Error::msg(op, "malformed null GUID"));
        }
        // SAFETY: GetValue owns one GUID until the property guard clears it.
        let guid = unsafe { *pointer };
        return stable_text(format!("{{{guid:?}}}"), op).map(Some);
    }
    if raw.vt != VT_LPWSTR {
        return Err(Error::msg(
            op,
            "unsupported property type (expected VT_LPWSTR)",
        ));
    }
    // SAFETY: The discriminator is VT_LPWSTR; the guard owns the string until return.
    let text = unsafe { string(raw.Anonymous.pwszVal, op) }?;
    if key == &STABLE_ID {
        return stable_text(text, op).map(Some);
    }
    Ok((!text.is_empty()).then_some(text))
}

fn adapter_instance_id(device: &IMMDevice, enumerator: &IMMDeviceEnumerator) -> Result<String> {
    // SAFETY: The endpoint is live in this apartment. Only topology is activated;
    // this does not activate an audio client/stream or change device state.
    let topology: IDeviceTopology = unsafe { device.Activate(CLSCTX_INPROC_SERVER, None) }
        .map_err(|e| api_error("IMMDevice::Activate(IDeviceTopology)", e))?;
    // SAFETY: The topology is live in this worker's apartment.
    let count = unsafe { topology.GetConnectorCount() }
        .map_err(|e| api_error("IDeviceTopology::GetConnectorCount", e))?;
    if count != 1 {
        return Err(Error::msg(
            "audio topology association",
            "ambiguous: expected exactly one adapter connector",
        ));
    }
    // SAFETY: Exactly one endpoint connector was verified above.
    let connector = unsafe { topology.GetConnector(0) }
        .map_err(|e| api_error("IDeviceTopology::GetConnector", e))?;
    // SAFETY: The live connector transfers a CoTaskMem-allocated interface-path string.
    let pointer = unsafe { connector.GetDeviceIdConnectedTo() }
        .map_err(|e| api_error("IConnector::GetDeviceIdConnectedTo", e))?;
    // SAFETY: A successful call owns a terminated string until CoTaskMemFree.
    let path = unsafe { string(pointer, "IConnector::GetDeviceIdConnectedTo") };
    // SAFETY: This allocation uses CoTaskMem, even if decoding failed.
    unsafe { CoTaskMemFree(Some(pointer.0.cast())) };
    let path = wide::to_wide(&path?);
    // SAFETY: GetDeviceIdConnectedTo returns an MMDevice ID accepted by GetDevice.
    // It may have a {2}. prefix; do not parse that opaque token as a SetupAPI path.
    let adapter = unsafe { enumerator.GetDevice(PCWSTR(path.as_ptr())) }
        .map_err(|e| api_error("IMMDeviceEnumerator::GetDevice (audio adapter)", e))?;
    // SAFETY: This reads the topology adapter's property store, not the endpoint's.
    let store = unsafe { adapter.OpenPropertyStore(STGM_READ) }
        .map_err(|e| api_error("IMMDevice::OpenPropertyStore (audio adapter)", e))?;
    if let Some(id) = property(
        &store,
        &PKEY_Device_InstanceId,
        "PKEY_Device_InstanceId (audio adapter)",
    )? {
        return adapter_id(id);
    }
    Err(Error::msg(
        "PKEY_Device_InstanceId (audio adapter)",
        "property absent or empty",
    ))
}

fn read_endpoint(
    device: &IMMDevice,
    enumerator: &IMMDeviceEnumerator,
    direction: &'static str,
    scan: &mut Scan,
    tx: &mpsc::Sender<Message>,
) {
    let mut endpoint = Endpoint {
        direction,
        id: None,
        name: None,
        adapter: None,
        instance_id: None,
        stable_id: None,
    };
    publish_endpoint(scan, &endpoint, tx);
    // SAFETY: device belongs to this apartment. GetId transfers a CoTaskMem string.
    match unsafe { device.GetId() } {
        Ok(pointer) => {
            // SAFETY: The successful call owns a live SDK-terminated UTF-16 string.
            let value = unsafe { string(pointer, "IMMDevice::GetId") };
            // SAFETY: GetId requires CoTaskMemFree, including when decoding failed.
            unsafe { CoTaskMemFree(Some(pointer.0.cast())) };
            match value {
                Ok(id) if !id.is_empty() => endpoint.id = Some(id),
                Ok(_) => scan
                    .failures
                    .push(Error::msg("IMMDevice::GetId", "empty endpoint ID")),
                Err(e) => scan.failures.push(e),
            }
        }
        Err(e) => scan.failures.push(api_error("IMMDevice::GetId", e)),
    }
    publish_endpoint(scan, &endpoint, tx);
    // SAFETY: Open only the read-only property store; no stream activation or writes.
    let store = match unsafe { device.OpenPropertyStore(STGM_READ) } {
        Ok(store) => {
            for (key, op, field) in [
                (
                    &PKEY_Device_FriendlyName,
                    "PKEY_Device_FriendlyName",
                    &mut endpoint.name,
                ),
                (
                    &PKEY_DeviceInterface_FriendlyName,
                    "PKEY_DeviceInterface_FriendlyName",
                    &mut endpoint.adapter,
                ),
                (
                    &PKEY_Device_InstanceId,
                    "PKEY_Device_InstanceId",
                    &mut endpoint.instance_id,
                ),
            ] {
                match property(&store, key, op) {
                    Ok(Some(value)) => *field = Some(value),
                    Ok(None) => scan
                        .failures
                        .push(Error::msg(op, "property absent or empty")),
                    Err(e) => scan.failures.push(e),
                }
            }
            Some(store)
        }
        Err(e) => {
            scan.failures
                .push(api_error("IMMDevice::OpenPropertyStore", e));
            None
        }
    };
    // Some property stores identify the endpoint's SWD devnode, not the adapter.
    // Never use that software devnode as an adapter grouping key.
    if let Some(id) = endpoint.instance_id.take() {
        match adapter_id(id) {
            Ok(id) => endpoint.instance_id = Some(id),
            Err(error) => scan.failures.push(error),
        }
    }
    publish_endpoint(scan, &endpoint, tx);
    if endpoint.instance_id.is_none() {
        match adapter_instance_id(device, enumerator) {
            Ok(id) => endpoint.instance_id = Some(id),
            Err(error) => scan.failures.push(error),
        }
    }
    // Publish existing fields before asking for the optional StableId. A stalled
    // property provider cannot erase the completed endpoint from the report.
    publish_endpoint(scan, &endpoint, tx);
    if let Some(store) = store {
        match property(&store, &STABLE_ID, "PKEY_AudioEndpoint_StableId") {
            Ok(Some(value)) => endpoint.stable_id = Some(value),
            Ok(None) => scan.failures.push(Error::msg(
                "PKEY_AudioEndpoint_StableId",
                "absent: property absent or empty",
            )),
            Err(error) => scan.failures.push(error),
        }
    }
    publish_endpoint(scan, &endpoint, tx);
    scan.endpoints.push(endpoint);
}

fn publish_endpoint(scan: &mut Scan, endpoint: &Endpoint, tx: &mpsc::Sender<Message>) {
    if !scan.failures.is_empty() {
        let _ = tx.send(Message::Failures(std::mem::take(&mut scan.failures)));
    }
    let _ = tx.send(Message::Endpoint(scan.endpoints.len(), endpoint.clone()));
}

/// Enumerates active render/capture endpoints; inactive counts are diagnostics only.
/// Waits at most five seconds; one worker retains its COM objects until calls return.
pub fn endpoints() -> Result<Scan> {
    let Some(worker) = Worker::acquire(&BUSY) else {
        return Err(Error::msg(
            "Core Audio",
            "timeout: previous scan still running",
        ));
    };
    let deadline = Instant::now() + BUDGET;
    let (tx, rx) = mpsc::channel();
    std::thread::Builder::new()
        .name("audio-endpoints".into())
        .spawn(move || {
            let _worker = worker;
            let result = match catch_panic(|| scan_endpoints(deadline, &tx)) {
                Ok(result) => result,
                Err(_) => Err(Error::msg("Core Audio", "malformed: worker panicked")),
            };
            let _ = tx.send(Message::Finished(result));
        })
        .map_err(|error| {
            Error::msg(
                "Core Audio worker",
                format!("unsupported: cannot spawn worker ({error})"),
            )
        })?;
    receive_scan(&rx, deadline)
}

fn receive_scan(rx: &mpsc::Receiver<Message>, deadline: Instant) -> Result<Scan> {
    let mut scan = Scan {
        incomplete: true,
        ..Scan::default()
    };
    loop {
        match rx.recv_timeout(deadline.saturating_duration_since(Instant::now())) {
            Ok(Message::Failures(failures)) => scan.failures.extend(failures),
            Ok(Message::Endpoint(index, endpoint)) => {
                if let Some(slot) = scan.endpoints.get_mut(index) {
                    *slot = endpoint;
                } else if index == scan.endpoints.len() && index < ENDPOINT_LIMIT as usize * 2 {
                    scan.endpoints.push(endpoint);
                } else {
                    scan.failures.push(Error::msg(
                        "Core Audio worker",
                        "malformed: endpoint sequence mismatch",
                    ));
                }
            }
            Ok(Message::Finished(Ok(mut finished))) => {
                finished.failures.extend(scan.failures);
                return Ok(finished);
            }
            Ok(Message::Finished(Err(error))) => {
                if scan.endpoints.is_empty() {
                    return Err(error);
                }
                scan.failures.push(error);
                return Ok(scan);
            }
            Err(error) => {
                let detail = match error {
                    mpsc::RecvTimeoutError::Timeout => {
                        "timeout: five-second scan deadline exceeded"
                    }
                    mpsc::RecvTimeoutError::Disconnected => {
                        "malformed: result channel disconnected"
                    }
                };
                scan.failures.push(Error::msg("Core Audio", detail));
                return Ok(scan);
            }
        }
    }
}

fn scan_endpoints(deadline: Instant, tx: &mpsc::Sender<Message>) -> Result<Scan> {
    let _apartment = Apartment::new()?;
    // SAFETY: COM is initialized; creation requests only the local enumerator interface.
    let enumerator: IMMDeviceEnumerator =
        unsafe { CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_INPROC_SERVER) }
            .map_err(|e| api_error("CoCreateInstance(MMDeviceEnumerator)", e))?;
    let mut scan = Scan::default();
    for (flow, direction) in [(eRender, "Render"), (eCapture, "Capture")] {
        if Instant::now() >= deadline {
            return Err(Error::msg("Core Audio", "timeout: scan deadline exceeded"));
        }
        // SAFETY: Enumerator belongs to this apartment; only active devices are requested.
        match unsafe { enumerator.EnumAudioEndpoints(flow, DEVICE_STATE_ACTIVE) } {
            Ok(devices) => {
                // SAFETY: The collection is live and owned on this thread.
                match unsafe { devices.GetCount() } {
                    Ok(count) => {
                        if count > ENDPOINT_LIMIT {
                            scan.incomplete = true;
                            scan.failures.push(Error::msg(
                                "IMMDeviceCollection::GetCount",
                                "malformed: endpoint count exceeds 4096",
                            ));
                            continue;
                        }
                        for index in 0..count {
                            if Instant::now() >= deadline {
                                return Err(Error::msg(
                                    "Core Audio",
                                    "timeout: scan deadline exceeded",
                                ));
                            }
                            // SAFETY: index is below the collection's advertised count.
                            match unsafe { devices.Item(index) } {
                                Ok(device) => {
                                    read_endpoint(&device, &enumerator, direction, &mut scan, tx)
                                }
                                Err(e) => {
                                    scan.incomplete = true;
                                    scan.failures
                                        .push(api_error("IMMDeviceCollection::Item", e));
                                }
                            }
                        }
                    }
                    Err(e) => {
                        scan.incomplete = true;
                        scan.failures
                            .push(api_error("IMMDeviceCollection::GetCount", e));
                    }
                }
            }
            Err(e) => {
                scan.incomplete = true;
                scan.failures.push(api_error(
                    "IMMDeviceEnumerator::EnumAudioEndpoints(active)",
                    e,
                ));
            }
        }
        // SAFETY: Diagnostic-only inventory of inactive devices; no endpoint properties read.
        match unsafe {
            enumerator.EnumAudioEndpoints(
                flow,
                DEVICE_STATE(
                    DEVICE_STATE_DISABLED.0 | DEVICE_STATE_NOTPRESENT.0 | DEVICE_STATE_UNPLUGGED.0,
                ),
            )
        } {
            Ok(devices) => {
                // SAFETY: Collection is live on this thread.
                match unsafe { devices.GetCount() } {
                    Ok(count) => scan.inactive.push((direction, count)),
                    Err(e) => scan
                        .failures
                        .push(api_error("IMMDeviceCollection::GetCount(inactive)", e)),
                }
            }
            Err(e) => scan.failures.push(api_error(
                "IMMDeviceEnumerator::EnumAudioEndpoints(inactive)",
                e,
            )),
        }
    }
    Ok(scan)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn audio_busy_guard_survives_caller_timeout_and_resets_on_worker_exit() {
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

    fn endpoint() -> Endpoint {
        Endpoint {
            direction: "Render",
            id: Some("endpoint-Q7F29D4".into()),
            name: Some("Acme Bluetooth Audio".into()),
            adapter: None,
            instance_id: None,
            stable_id: None,
        }
    }

    #[test]
    fn audio_text_stable_guid_and_root_adapter_validation() {
        assert_eq!(
            validated_text(&[65, 233], "audio property").expect("UTF16"),
            "Aé"
        );
        assert_eq!(
            stable_text("audio-device-Q7f29B4a".into(), "Stable ID").expect("stable"),
            "audio-device-Q7f29B4a"
        );
        for bad in [vec![0xd800], vec![65, 10]] {
            assert!(validated_text(&bad, "audio property").is_err());
        }
        for bad in [
            "",
            "  ",
            "unknown",
            "00000000",
            "FFFFFFFF",
            "{00000000-0000-0000-0000-000000000000}",
            "{FFFFFFFF-FFFF-FFFF-FFFF-FFFFFFFFFFFF}",
        ] {
            assert!(stable_text(bad.into(), "Stable ID").is_err());
        }
        for good in [
            r"USB\VID_046D&PID_0A9F\A7C28E41",
            r"BTHENUM\DEV_001A7D7F1234\8&2B3C&0",
            r"ROOT\MEDIA\0007",
        ] {
            assert_eq!(adapter_id(good.into()).expect("adapter"), good);
        }
        for bad in [
            "",
            "ROOT",
            r"HTREE\ROOT\0",
            r"SWD\MMDEVAPI\endpoint-Q7F29D4",
            "USB\\node\\\n",
        ] {
            assert!(adapter_id(bad.into()).is_err());
        }
    }

    #[test]
    fn audio_worker_timeout_failure_and_success_preserve_completed_endpoint() {
        for failure in [false, true] {
            let (tx, rx) = mpsc::channel();
            tx.send(Message::Endpoint(0, endpoint())).expect("endpoint");
            if failure {
                tx.send(Message::Finished(Err(Error::msg(
                    "Stable ID",
                    "access-denied: fabricated failure",
                ))))
                .expect("failure");
            }
            let scan = receive_scan(&rx, Instant::now()).expect("partial scan");
            assert_eq!(scan.endpoints.len(), 1);
            assert_eq!(
                scan.endpoints[0].name.as_deref(),
                Some("Acme Bluetooth Audio")
            );
            assert!(scan.endpoints[0].stable_id.is_none());
            assert_eq!(scan.failures.len(), 1);
            assert!(scan.incomplete);
        }
        let (tx, rx) = mpsc::channel();
        let mut ep = endpoint();
        ep.stable_id = Some("audio-device-Q7f29B4a".into());
        tx.send(Message::Finished(Ok(Scan {
            endpoints: vec![ep],
            ..Scan::default()
        })))
        .expect("complete");
        let scan = receive_scan(&rx, Instant::now()).expect("scan");
        assert!(!scan.incomplete);
        assert!(scan.endpoints[0].stable_id.is_some());
        let (tx, rx) = mpsc::channel();
        tx.send(Message::Endpoint(9, endpoint()))
            .expect("malformed sequence");
        drop(tx);
        let scan = receive_scan(&rx, Instant::now()).expect("scan");
        assert!(scan.endpoints.is_empty());
        assert_eq!(scan.failures.len(), 2);
    }

    #[test]
    fn audio_com_apartments_balance_mta_and_existing_sta() {
        // Real worker-local COM initialization; no audio inventory or identifiers.
        std::thread::spawn(|| {
            let first = Apartment::new().expect("MTA");
            let second = Apartment::new().expect("existing MTA");
            drop(second);
            drop(first);
            // SAFETY: Fresh test worker requests its own STA, balanced below.
            unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) }
                .ok()
                .expect("STA");
            let retained = Apartment::new().expect("retain STA");
            drop(retained);
            // SAFETY: Balances the explicit STA initialization above.
            unsafe { CoUninitialize() };
        })
        .join()
        .expect("COM worker");
    }
}
