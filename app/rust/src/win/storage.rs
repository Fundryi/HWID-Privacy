//! Read-only disk queries and checked SDK-layout parsers for WP-01.

use super::{Error, Result, ioctl, wide};
use std::{
    mem::{offset_of, size_of},
    panic::{AssertUnwindSafe, catch_unwind},
    sync::{
        Mutex,
        atomic::{AtomicUsize, Ordering},
        mpsc,
    },
    thread,
    time::{Duration, Instant},
};
use windows::Win32::{
    Foundation::{GENERIC_READ, GENERIC_WRITE},
    Storage::FileSystem::{
        BusTypeAta, BusTypeNvme, BusTypeSata, GetDriveTypeW, GetLogicalDrives,
        GetVolumeInformationW, IOCTL_VOLUME_GET_VOLUME_DISK_EXTENTS,
    },
    System::{
        Diagnostics::Debug::{SEM_FAILCRITICALERRORS, SetThreadErrorMode},
        Ioctl::*,
    },
};
use windows::core::PCWSTR;

/// The selected storage identifier, preserving raw bytes and C# ASCII decoding.
#[derive(Debug, PartialEq, Eq)]
pub struct Identifier {
    /// Uppercase colon-separated bytes, including binary 16-byte identifiers.
    pub hex: String,
    /// ASCII text when the code set or printable-byte check permits it.
    pub decoded: String,
}

/// Full VPD reply alongside the unchanged legacy-selected identifier.
pub struct StorageIdentifiers {
    pub selected: Option<Identifier>,
    pub descriptors: IdentifyOutcome<Vec<StorageIdentifier>>,
}

/// One checked STORAGE_IDENTIFIER; SDK Type is STORAGE_IDENTIFIER_TYPE.
#[derive(Debug, PartialEq, Eq)]
pub struct StorageIdentifier {
    pub code_set: u32,
    pub identifier_type: u32,
    pub association: u32,
    pub value: Vec<u8>,
}

/// Disk identity and GPT partition identities in legacy display notation.
#[derive(Debug, PartialEq, Eq)]
pub struct DiskLayout {
    /// True for GPT, false for MBR.
    pub is_gpt: bool,
    /// Uppercase D-format GUID or the MBR signature with its 0x prefix.
    pub disk_id: String,
    /// Nonzero GPT partition GUIDs in on-disk order.
    pub partition_guids: Vec<String>,
}

/// One successfully mapped fixed or removable volume.
pub struct Volume {
    /// Physical disk number returned by both native mapping queries.
    pub disk_number: u32,
    /// Eight uppercase hexadecimal volume-serial digits.
    pub serial: String,
}

// The frozen synchronous IOCTL helper cannot cancel a driver call. Abandon only
// the wait; the worker retains ownership of its buffers/handles until it returns.
const DISK_BUDGET: Duration = Duration::from_secs(5);
const DISK_JOBS: usize = 4;
pub const MAX_DISKS: usize = 256;
// Four NVMe disks can issue five independent requests each. Reserve four slots
// for overlapping volume work, and retain occupied slots across timed-out refreshes.
const IO_JOBS: usize = 24;
static IO_ACTIVE: AtomicUsize = AtomicUsize::new(0);
static ATA_PASSTHROUGH: Mutex<Vec<u32>> = Mutex::new(Vec::new());

struct IoWorker;
impl Drop for IoWorker {
    fn drop(&mut self) {
        IO_ACTIVE.fetch_sub(1, Ordering::Release);
    }
}

struct AtaWorker(u32);
impl Drop for AtaWorker {
    fn drop(&mut self) {
        ATA_PASSTHROUGH
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .retain(|index| *index != self.0);
    }
}

impl AtaWorker {
    fn acquire(index: u32) -> Result<Self> {
        let mut active = ATA_PASSTHROUGH.lock().unwrap_or_else(|e| e.into_inner());
        if active.contains(&index) {
            return Err(Error::msg(
                "ATA Identify",
                "ambiguous: previous ATA passthrough still running",
            ));
        }
        active.push(index);
        Ok(Self(index))
    }
}

fn bounded<T: Send + 'static>(
    op: &'static str,
    deadline: Instant,
    work: impl FnOnce() -> Result<T> + Send + 'static,
) -> Result<T> {
    if Instant::now() >= deadline {
        return Err(Error::msg(op, "timed out after 5000 ms"));
    }
    if !reserve_io_worker(&IO_ACTIVE) {
        return Err(Error::msg(
            op,
            "timeout: previous storage queries still running",
        ));
    }
    let worker = IoWorker;
    let (send, receive) = mpsc::sync_channel(1);
    thread::Builder::new()
        .name(format!("storage {op}"))
        .spawn(move || {
            let _worker = worker;
            let result = catch_unwind(AssertUnwindSafe(|| {
                // SAFETY: This is a dedicated, short-lived OS-query thread. Its error
                // mode disappears with it; no caller or pooled thread is modified.
                unsafe { SetThreadErrorMode(SEM_FAILCRITICALERRORS, None) }
                    .map_err(|e| Error::from_win("SetThreadErrorMode", e))?;
                if Instant::now() >= deadline {
                    return Err(Error::msg(op, "timed out after 5000 ms"));
                }
                work()
            }))
            .unwrap_or_else(|_| Err(Error::msg(op, "storage worker panicked")));
            // The caller may have timed out; dropping a late result also drops handles.
            let _ = send.send(result);
        })
        .map_err(|e| Error::msg(op, format!("start worker: {e}")))?;
    receive
        .recv_timeout(deadline.saturating_duration_since(Instant::now()))
        .map_err(|e| {
            Error::msg(
                op,
                match e {
                    mpsc::RecvTimeoutError::Timeout => "timed out after 5000 ms",
                    mpsc::RecvTimeoutError::Disconnected => "storage worker disconnected",
                },
            )
        })?
}

fn reserve_io_worker(active: &AtomicUsize) -> bool {
    active
        .fetch_update(Ordering::AcqRel, Ordering::Acquire, |n| {
            (n < IO_JOBS).then(|| n.saturating_add(1))
        })
        .is_ok()
}

/// Independently retained field groups for one physical disk.
pub struct PhysicalQueries {
    pub nvme: Result<NvmeIdentity>,
    pub ata: Option<Result<AtaIdentity>>,
    pub identifiers: Result<StorageIdentifiers>,
    pub layout: Result<Option<DiskLayout>>,
}

impl PhysicalQueries {
    pub fn unavailable() -> Self {
        let error = Error::msg("storage worker", "disk job did not complete");
        Self {
            nvme: Err(error.clone()),
            ata: None,
            identifiers: Err(error.clone()),
            layout: Err(error),
        }
    }
}

/// Four disk jobs at a time, returned in input order. Every job shares one 5 s
/// deadline across protocol, VPD and layout, including ATA fallback. Scope joins
/// only bounded callers; detached OS workers continue owning their own handles.
pub fn physical_queries(indices: &[u32]) -> Vec<PhysicalQueries> {
    if indices.len() > MAX_DISKS {
        super::record(Error::msg(
            "storage worker",
            "implausible: disk count exceeds collection cap",
        ));
        return (0..indices.len())
            .map(|_| PhysicalQueries::unavailable())
            .collect();
    }
    disk_jobs(indices, &|index, deadline| {
        thread::scope(|scope| {
            let identifiers = scope.spawn(|| physical_identifier_until(index, deadline));
            let layout = scope.spawn(|| physical_layout_until(index, deadline));
            let nvme = physical_nvme_identity_until(index, deadline);
            let ata = match &nvme {
                Ok(NvmeIdentity {
                    controller_serial: IdentifyOutcome::NotAttempted { bus },
                    ..
                }) => Some(physical_ata_identity_until(index, *bus, deadline)),
                _ => None,
            };
            PhysicalQueries {
                nvme,
                ata,
                identifiers: identifiers.join().unwrap_or_else(|_| {
                    Err(Error::msg(
                        "StorageDeviceIdProperty",
                        "storage worker panicked",
                    ))
                }),
                layout: layout.join().unwrap_or_else(|_| {
                    Err(Error::msg(
                        "IOCTL_DISK_GET_DRIVE_LAYOUT_EX",
                        "storage worker panicked",
                    ))
                }),
            }
        })
    })
}

// A large disk set must not keep a detached section worker scheduling IO forever.
// Each disk keeps its own 5 s budget within a 50 s batch caller-wait ceiling.
fn disk_jobs(
    indices: &[u32],
    query: &(impl Fn(u32, Instant) -> PhysicalQueries + Sync),
) -> Vec<PhysicalQueries> {
    let batch_deadline = Instant::now() + Duration::from_secs(50);
    let next = AtomicUsize::new(0);
    let results = Mutex::new((0..indices.len()).map(|_| None).collect::<Vec<_>>());
    thread::scope(|scope| {
        for _ in 0..DISK_JOBS.min(indices.len()) {
            scope.spawn(|| {
                loop {
                    let position = next.fetch_add(1, Ordering::Relaxed);
                    let Some(&index) = indices.get(position) else {
                        break;
                    };
                    let deadline = (Instant::now() + DISK_BUDGET).min(batch_deadline);
                    let queries = if Instant::now() >= deadline {
                        super::record(Error::msg(
                            "storage worker",
                            "timeout: disk batch budget exhausted",
                        ));
                        PhysicalQueries::unavailable()
                    } else {
                        catch_unwind(AssertUnwindSafe(|| query(index, deadline))).unwrap_or_else(
                            |_| {
                                super::record(Error::msg(
                                    "storage worker",
                                    "malformed: disk job panicked",
                                ));
                                PhysicalQueries::unavailable()
                            },
                        )
                    };
                    if let Some(slot) = results
                        .lock()
                        .unwrap_or_else(|e| e.into_inner())
                        .get_mut(position)
                    {
                        *slot = Some(queries);
                    }
                }
            });
        }
    });
    results
        .into_inner()
        .unwrap_or_else(|e| e.into_inner())
        .into_iter()
        .map(|result| result.unwrap_or_else(PhysicalQueries::unavailable))
        .collect()
}

/// Enumerates drive letters in ascending order without probing network volumes.
pub fn drive_letters() -> Result<Vec<char>> {
    // SAFETY: GetLogicalDrives takes no pointers and returns a drive-letter mask.
    let mask = unsafe { GetLogicalDrives() };
    if mask == 0 {
        return Err(Error::last("GetLogicalDrives"));
    }
    Ok((0..26)
        .filter(|i| mask & (1 << i) != 0)
        .map(|i| char::from(b'A' + i))
        .collect())
}

/// Maps a fixed/removable letter; ambiguous or unsupported mappings require WMI.
pub fn volume(letter: char) -> Result<Option<Volume>> {
    if !letter.is_ascii_uppercase() {
        return Err(Error::msg("volume", "invalid drive letter"));
    }
    bounded("volume", Instant::now() + DISK_BUDGET, move || {
        let root = wide::to_wide(&format!("{letter}:\\"));
        // SAFETY: root is a live, NUL-terminated UTF-16 path.
        let kind = unsafe { GetDriveTypeW(PCWSTR(root.as_ptr())) };
        // DRIVE_REMOVABLE=2, DRIVE_FIXED=3 (winbase.h). The constants' generated
        // WindowsProgramming module is not enabled in the frozen Cargo features.
        if !matches!(kind, 2 | 3) {
            return Ok(None);
        }
        let handle = ioctl::open_device(&format!(r"\\.\{letter}:"), 0)?;
        let number = ioctl::device_io_control(
            &handle,
            IOCTL_STORAGE_GET_DEVICE_NUMBER,
            &[],
            size_of::<STORAGE_DEVICE_NUMBER>(),
        )?;
        let disk_number = dword(&number, offset_of!(STORAGE_DEVICE_NUMBER, DeviceNumber))?;
        let extents =
            ioctl::device_io_control(&handle, IOCTL_VOLUME_GET_VOLUME_DISK_EXTENTS, &[], 4096)?;
        if single_extent_disk(&extents)? != Some(disk_number) {
            return Err(Error::msg(
                "volume mapping",
                "volume spans disks or has an ambiguous device number; WMI associations required",
            ));
        }
        let mut serial = 0;
        // SAFETY: root stays live and serial is the only writable output requested.
        unsafe {
            GetVolumeInformationW(
                PCWSTR(root.as_ptr()),
                None,
                Some(&mut serial),
                None,
                None,
                None,
            )
        }
        .map_err(|e| Error::from_win("GetVolumeInformationW", e))?;
        Ok(Some(Volume {
            disk_number,
            serial: format!("{serial:08X}"),
        }))
    })
}

/// Reads the selected VPD identifier through the shared device/IOCTL helpers.
pub fn physical_identifier(index: u32) -> Result<StorageIdentifiers> {
    physical_identifier_until(index, Instant::now() + DISK_BUDGET)
}

fn physical_identifier_until(index: u32, deadline: Instant) -> Result<StorageIdentifiers> {
    let started = Instant::now();
    let data = bounded("StorageDeviceIdProperty", deadline, move || {
        // C# parity: Services/Win32/StorageDeviceIdQuery.cs:202-211 (read access).
        let handle = ioctl::open_device(&format!(r"\\.\PHYSICALDRIVE{index}"), GENERIC_READ.0)?;
        // Include the SDK AdditionalParameters tail, with zero-initialized padding.
        let mut query = vec![0; size_of::<STORAGE_PROPERTY_QUERY>()];
        let offset = offset_of!(STORAGE_PROPERTY_QUERY, PropertyId);
        query[offset..offset + 4].copy_from_slice(&StorageDeviceIdProperty.0.to_le_bytes());
        let offset = offset_of!(STORAGE_PROPERTY_QUERY, QueryType);
        query[offset..offset + 4].copy_from_slice(&PropertyStandardQuery.0.to_le_bytes());
        ioctl::device_io_control(&handle, IOCTL_STORAGE_QUERY_PROPERTY, &query, 4096)
    });
    let returned = data.as_ref().ok().map(Vec::len);
    let result = data.and_then(|data| {
        // Do not change the compatibility selector's ranking, decoding or stops.
        let selected = parse_descriptor(&data)?;
        let descriptors = parse_identifier_list(&data);
        storage_diagnostic(
            index,
            None,
            &descriptors.as_ref().map_or_else(
                |_| "StorageDeviceIdProperty descriptor list".into(),
                |ids| {
                    format!(
                        "StorageDeviceIdProperty descriptor list count={}",
                        ids.len()
                    )
                },
            ),
            returned,
            started,
            descriptors.as_ref().err(),
            if matches!(&descriptors, Ok(ids) if ids.is_empty()) {
                "empty"
            } else {
                "ok"
            },
        );
        Ok(StorageIdentifiers {
            selected,
            descriptors: identify_outcome(descriptors.map(|ids| (!ids.is_empty()).then_some(ids))),
        })
    });
    if let Err(error) = &result {
        storage_diagnostic(
            index,
            None,
            "StorageDeviceIdProperty",
            returned,
            started,
            Some(error),
            "ok",
        );
    }
    result
}

/// Independently collected controller and namespace identities, never Windows Serial.
pub struct NvmeIdentity {
    /// Identify Controller SN, with fixed-width padding removed.
    pub controller_serial: IdentifyOutcome<String>,
    /// Identify Namespace binary identities in their original byte order.
    pub namespace: IdentifyOutcome<NvmeNamespace>,
    /// CNS 3 descriptors, independent of controller and CNS 0 results.
    pub namespace_descriptors: IdentifyOutcome<NvmeNamespaceDescriptors>,
}

/// A protocol field group, retaining empty and bus-gated results separately.
pub enum IdentifyOutcome<T> {
    Ok(T),
    Failed(Error),
    Empty,
    NotAttempted { bus: u32 },
}

/// Optional namespace identities; all-zero fields mean not supplied.
#[derive(Debug, PartialEq, Eq)]
pub struct NvmeNamespace {
    /// Uppercase EUI-64, without separators.
    pub eui64: Option<String>,
    /// Uppercase NGUID, without separators.
    pub nguid: Option<String>,
}

/// Namespace Identification Descriptor List identities in protocol byte order.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct NvmeNamespaceDescriptors {
    pub eui64: Option<String>,
    pub nguid: Option<String>,
    /// RFC 4122 byte order, unlike a Windows mixed-endian GUID.
    pub uuid: Option<String>,
    /// Command set context, not an identity value.
    pub csi: Option<u8>,
}

/// ATA fields are independent: malformed text cannot hide a valid WWN or model.
pub struct AtaIdentity {
    /// Word-swapped serial; padding and recognized placeholders are omitted.
    pub serial: IdentifyOutcome<String>,
    /// Independently parsed model context.
    pub model: IdentifyOutcome<String>,
    /// Independently parsed firmware context.
    pub firmware: IdentifyOutcome<String>,
    /// Numeric words 108–111, present only when WWN support/validity permit it.
    pub wwn: IdentifyOutcome<u64>,
}

const ATA_IDENTIFY_SIZE: usize = 512;
// ntddscsi.h: CTL_CODE(IOCTL_SCSI_BASE=4, 0x040b, METHOD_BUFFERED, READ|WRITE).
const ATA_PASS_THROUGH_IOCTL: u32 = 0x0004_D02C;

// ATA_PASS_THROUGH_EX from ntddscsi.h. The SDK type is in an unenabled WDK
// feature; this layout supplies checked offsets, never a cast into a byte buffer.
#[repr(C)]
struct AtaPassThrough {
    length: u16,
    flags: u16,
    path: u8,
    target: u8,
    lun: u8,
    reserved_byte: u8,
    transfer_length: u32,
    timeout: u32,
    reserved: u32,
    data_offset: usize,
    previous_task_file: [u8; 8],
    current_task_file: [u8; 8],
}

/// Reads ATA IDENTIFY using the already validated StorageDeviceProperty bus.
/// Only the fallback opens read/write access, and its sole command is read-only 0xEC.
pub fn physical_ata_identity(index: u32, bus: u32) -> Result<AtaIdentity> {
    physical_ata_identity_until(index, bus, Instant::now() + DISK_BUDGET)
}

fn physical_ata_identity_until(index: u32, bus: u32, deadline: Instant) -> Result<AtaIdentity> {
    if bus != BusTypeAta.0 as u32 && bus != BusTypeSata.0 as u32 {
        storage_diagnostic(
            index,
            Some(bus),
            "ATA Identify (native ATA/SATA only; USB/SAT and RAID excluded)",
            None,
            Instant::now(),
            None,
            "not-attempted",
        );
        return Ok(AtaIdentity {
            serial: IdentifyOutcome::NotAttempted { bus },
            model: IdentifyOutcome::NotAttempted { bus },
            firmware: IdentifyOutcome::NotAttempted { bus },
            wwn: IdentifyOutcome::NotAttempted { bus },
        });
    }
    let read = |passthrough| {
        let started = Instant::now();
        let data = bounded("ATA Identify", deadline, move || {
            // Hold exclusion inside the OS worker, even after its caller times out.
            // A subsequent refresh must never overlap passthrough on this disk.
            let _ata = if passthrough {
                Some(AtaWorker::acquire(index)?)
            } else {
                None
            };
            let access = if passthrough {
                GENERIC_READ.0 | GENERIC_WRITE.0
            } else {
                GENERIC_READ.0
            };
            let handle = ioctl::open_device(&format!(r"\\.\PHYSICALDRIVE{index}"), access)?;
            if passthrough {
                let query = ata_passthrough_query();
                ioctl::device_io_control(&handle, ATA_PASS_THROUGH_IOCTL, &query, 1024)
            } else {
                let mut query = [0; PROTOCOL_START + size_of::<STORAGE_PROTOCOL_SPECIFIC_DATA>()];
                for (offset, value) in [
                    (0, StorageDeviceProtocolSpecificProperty.0 as u32),
                    (4, PropertyStandardQuery.0 as u32),
                    (8, ProtocolTypeAta.0 as u32),
                    (12, AtaDataTypeIdentify.0 as u32),
                    (24, size_of::<STORAGE_PROTOCOL_SPECIFIC_DATA>() as u32),
                    (28, ATA_IDENTIFY_SIZE as u32),
                ] {
                    query[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
                }
                ioctl::device_io_control(&handle, IOCTL_STORAGE_QUERY_PROPERTY, &query, 1024)
            }
        });
        let returned = data.as_ref().ok().map(Vec::len);
        let result = data.and_then(|data| {
            let payload = if passthrough {
                ata_passthrough_payload(&data)?
            } else {
                ata_protocol_payload(&data)?
            };
            parse_ata_identity(payload)
        });
        storage_diagnostic(
            index,
            Some(bus),
            if passthrough {
                "IOCTL_ATA_PASS_THROUGH command=0xEC"
            } else {
                "StorageDeviceProtocolSpecificProperty ATA Identify"
            },
            returned,
            started,
            result.as_ref().err(),
            "ok",
        );
        result
    };
    // Preserve the first failure in helper diagnostics even if fallback succeeds.
    // A field-level parse failure does not discard other fields or reread the disk.
    read(false).or_else(|_| read(true))
}

fn ata_passthrough_query() -> Vec<u8> {
    let header = size_of::<AtaPassThrough>();
    let mut query = vec![0; header];
    query[..2].copy_from_slice(&(header as u16).to_le_bytes());
    // ATA_FLAGS_DRDY_REQUIRED | ATA_FLAGS_DATA_IN. No DATA_OUT or DMA flag.
    query[2..4].copy_from_slice(&3u16.to_le_bytes());
    let offset = offset_of!(AtaPassThrough, transfer_length);
    query[offset..offset + 4].copy_from_slice(&(ATA_IDENTIFY_SIZE as u32).to_le_bytes());
    let offset = offset_of!(AtaPassThrough, timeout);
    query[offset..offset + 4].copy_from_slice(&3u32.to_le_bytes());
    let offset = offset_of!(AtaPassThrough, data_offset);
    query[offset..offset + size_of::<usize>()].copy_from_slice(&header.to_le_bytes());
    let task = offset_of!(AtaPassThrough, current_task_file);
    query[task + 1] = 1; // One 512-byte sector.
    query[task + 6] = 0xEC; // IDENTIFY DEVICE; never accept a caller-supplied command.
    query
}

fn ata_passthrough_payload(data: &[u8]) -> Result<&[u8]> {
    let header = size_of::<AtaPassThrough>();
    let offset = usize::from_le_bytes(bytes(data, offset_of!(AtaPassThrough, data_offset))?);
    if word(data, 0)? as usize != header
        || dword(data, offset_of!(AtaPassThrough, transfer_length))? as usize != ATA_IDENTIFY_SIZE
        || offset != header
    {
        return Err(Error::msg(
            "ATA Identify",
            "invalid passthrough length or offset",
        ));
    }
    let task = bytes::<8>(data, offset_of!(AtaPassThrough, current_task_file))?;
    // ATA status: reject BSY, DF, DRQ and ERR, and require device-ready completion.
    if task[6] & 0xA9 != 0 || task[6] & 0x40 == 0 {
        return Err(Error::msg(
            "ATA task file",
            "IDENTIFY DEVICE did not complete successfully",
        ));
    }
    data.get(offset..offset.saturating_add(ATA_IDENTIFY_SIZE))
        .ok_or_else(|| Error::msg("ATA Identify", "invalid truncated passthrough payload"))
}

fn ata_protocol_payload(data: &[u8]) -> Result<&[u8]> {
    let header = size_of::<STORAGE_PROTOCOL_DATA_DESCRIPTOR>() as u32;
    let offset = dword(data, 24)? as usize;
    if dword(data, 0)? != header
        || dword(data, 4)? != header
        || dword(data, 8)? != ProtocolTypeAta.0 as u32
        || dword(data, 12)? != AtaDataTypeIdentify.0 as u32
        || offset < size_of::<STORAGE_PROTOCOL_SPECIFIC_DATA>()
        || dword(data, 28)? as usize != ATA_IDENTIFY_SIZE
    {
        return Err(Error::msg("ATA Identify", "invalid protocol descriptor"));
    }
    PROTOCOL_START
        .checked_add(offset)
        .and_then(|start| {
            start
                .checked_add(ATA_IDENTIFY_SIZE)
                .and_then(|end| data.get(start..end))
        })
        .ok_or_else(|| Error::msg("ATA Identify", "invalid protocol payload bounds"))
}

fn identify_outcome<T>(result: Result<Option<T>>) -> IdentifyOutcome<T> {
    match result {
        Ok(Some(value)) => IdentifyOutcome::Ok(value),
        Ok(None) => IdentifyOutcome::Empty,
        Err(error) => IdentifyOutcome::Failed(error),
    }
}

fn parse_ata_identity(payload: &[u8]) -> Result<AtaIdentity> {
    if payload.len() != ATA_IDENTIFY_SIZE {
        return Err(Error::msg(
            "ATA Identify",
            "invalid Identify payload length",
        ));
    }
    // Word 255: optional integrity signature 0xA5, then checksum over all 512 bytes.
    if payload.get(510) == Some(&0xA5)
        && payload.iter().fold(0u8, |sum, b| sum.wrapping_add(*b)) != 0
    {
        return Err(Error::msg(
            "ATA Identify",
            "invalid Identify integrity checksum",
        ));
    }
    Ok(AtaIdentity {
        serial: identify_outcome(ata_string(payload, 10, 20, true)),
        model: identify_outcome(ata_string(payload, 27, 40, false)),
        firmware: identify_outcome(ata_string(payload, 23, 8, false)),
        wwn: identify_outcome(ata_wwn(payload)),
    })
}

fn ata_string(
    payload: &[u8],
    start_word: usize,
    length: usize,
    serial: bool,
) -> Result<Option<String>> {
    let start = start_word
        .checked_mul(2)
        .ok_or_else(|| Error::msg("ATA Identify", "invalid string offset"))?;
    let end = start
        .checked_add(length)
        .ok_or_else(|| Error::msg("ATA Identify", "invalid string length"))?;
    let mut value = payload
        .get(start..end)
        .ok_or_else(|| Error::msg("ATA Identify", "invalid string bounds"))?
        .to_vec();
    for pair in value.as_chunks_mut::<2>().0 {
        pair.swap(0, 1);
    }
    let start = value
        .iter()
        .position(|b| !matches!(b, 0 | b' '))
        .unwrap_or(length);
    let end = value
        .iter()
        .rposition(|b| !matches!(b, 0 | b' '))
        .map_or(start, |i| i + 1);
    let value = value
        .get(start..end)
        .ok_or_else(|| Error::msg("ATA Identify", "invalid trimmed string bounds"))?;
    if value.iter().any(|b| !(0x20..=0x7e).contains(b)) {
        return Err(Error::msg(
            "ATA Identify",
            "invalid Identify string encoding",
        ));
    }
    let text: String = value.iter().map(|&b| char::from(b)).collect();
    let placeholder = serial
        && (matches!(
            text.to_ascii_uppercase().as_str(),
            "UNKNOWN"
                | "UNKNOWN SERIAL"
                | "NONE"
                | "N/A"
                | "NA"
                | "NOT SPECIFIED"
                | "NOT AVAILABLE"
                | "DEFAULT STRING"
                | "TO BE FILLED BY O.E.M."
        ) || value.iter().all(|&b| b == b'0')
            || value.iter().all(|&b| b == b'F' || b == b'f'));
    if text.is_empty() || placeholder {
        super::record(Error::msg(
            "ATA Identify string",
            if placeholder {
                "placeholder: serial omitted"
            } else {
                "absent: string omitted"
            },
        ));
        return Ok(None);
    }
    if serial
        && value
            .first()
            .is_some_and(|first| value.iter().all(|b| b == first))
    {
        super::record(Error::msg(
            "ATA Identify serial",
            "implausible: repeated character",
        ));
        return Ok(None);
    }
    Ok(Some(text))
}

fn ata_wwn(payload: &[u8]) -> Result<Option<u64>> {
    let support = word(payload, 84 * 2)?;
    let active = word(payload, 87 * 2)?;
    if support & 0xC000 != 0x4000
        || support & 0x0100 == 0
        || active & 0xC000 != 0x4000
        || active & 0x0100 == 0
    {
        super::record(Error::msg(
            "ATA WWN",
            "unsupported: WWN support or validity bits absent",
        ));
        return Ok(None);
    }
    let mut wwn = 0u64;
    for index in 108..112 {
        wwn = (wwn << 16) | u64::from(word(payload, index * 2)?);
    }
    if wwn == 0 || wwn == u64::MAX {
        super::record(Error::msg(
            "ATA WWN",
            "implausible: zero or all-FF identity",
        ));
        return Ok(None);
    }
    Ok(Some(wwn))
}

/// Reads NVMe Identify through storage property queries, gated by bus.
pub fn physical_nvme_identity(index: u32) -> Result<NvmeIdentity> {
    physical_nvme_identity_until(index, Instant::now() + DISK_BUDGET)
}

fn physical_nvme_identity_until(index: u32, deadline: Instant) -> Result<NvmeIdentity> {
    let started = Instant::now();
    let data = bounded("StorageDeviceProperty", deadline, move || {
        let handle = ioctl::open_device(&format!(r"\\.\PHYSICALDRIVE{index}"), GENERIC_READ.0)?;
        ioctl::device_io_control(
            &handle,
            IOCTL_STORAGE_QUERY_PROPERTY,
            &[0; size_of::<STORAGE_PROPERTY_QUERY>()],
            4096,
        )
    });
    let returned = data.as_ref().ok().map(Vec::len);
    let bus = data.and_then(|data| {
        let size = dword(&data, offset_of!(STORAGE_DEVICE_DESCRIPTOR, Size))? as usize;
        let bus_end = offset_of!(STORAGE_DEVICE_DESCRIPTOR, BusType) + 4;
        let version = dword(&data, offset_of!(STORAGE_DEVICE_DESCRIPTOR, Version))? as usize;
        if size < size_of::<STORAGE_DEVICE_DESCRIPTOR>()
            || size > data.len()
            || version < bus_end
            || version > size
        {
            return Err(Error::msg(
                "storage descriptor",
                "invalid device descriptor size",
            ));
        }
        dword(&data, offset_of!(STORAGE_DEVICE_DESCRIPTOR, BusType))
    });
    storage_diagnostic(
        index,
        bus.as_ref().ok().copied(),
        "StorageDeviceProperty CNS=n/a",
        returned,
        started,
        bus.as_ref().err(),
        "ok",
    );
    let bus = bus?;
    if bus != BusTypeNvme.0 as u32 {
        storage_diagnostic(
            index,
            Some(bus),
            "NVMe Identify CNS=1/0 SubValue=0",
            None,
            Instant::now(),
            None,
            "not-attempted",
        );
        return Ok(NvmeIdentity {
            controller_serial: IdentifyOutcome::NotAttempted { bus },
            namespace: IdentifyOutcome::NotAttempted { bus },
            namespace_descriptors: IdentifyOutcome::NotAttempted { bus },
        });
    }
    // Start independent groups together; one hung request cannot consume another
    // group's wait. Keep SubValue=0; multi-namespace association is unverified.
    let (controller_serial, namespace, mut namespace_descriptors) = thread::scope(|scope| {
        let controller = scope.spawn(|| nvme_identify(index, 1, deadline, parse_nvme_serial));
        let descriptors = scope.spawn(|| {
            nvme_identify(index, 3, deadline, |data| {
                let ids = parse_nvme_namespace_descriptors(data)?;
                Ok((ids.eui64.is_some()
                    || ids.nguid.is_some()
                    || ids.uuid.is_some()
                    || ids.csi.is_some())
                .then_some(ids))
            })
        });
        let namespace = nvme_identify(index, 0, deadline, |data| {
            let namespace = parse_nvme_namespace(data)?;
            Ok((namespace.eui64.is_some() || namespace.nguid.is_some()).then_some(namespace))
        });
        (
            controller.join().unwrap_or_else(|_| {
                IdentifyOutcome::Failed(Error::msg("NVMe Identify", "storage worker panicked"))
            }),
            namespace,
            descriptors.join().unwrap_or_else(|_| {
                IdentifyOutcome::Failed(Error::msg("NVMe Identify", "storage worker panicked"))
            }),
        )
    });
    validate_namespace_association(index, bus, started, &namespace, &mut namespace_descriptors);
    Ok(NvmeIdentity {
        controller_serial,
        namespace,
        namespace_descriptors,
    })
}

fn validate_namespace_association(
    index: u32,
    bus: u32,
    started: Instant,
    namespace: &IdentifyOutcome<NvmeNamespace>,
    namespace_descriptors: &mut IdentifyOutcome<NvmeNamespaceDescriptors>,
) {
    let conflict = if let (IdentifyOutcome::Ok(namespace), IdentifyOutcome::Ok(ids)) =
        (namespace, &*namespace_descriptors)
    {
        [
            (&namespace.eui64, &ids.eui64),
            (&namespace.nguid, &ids.nguid),
        ]
        .into_iter()
        .any(|(fixed, descriptor)| matches!((fixed, descriptor), (Some(a), Some(b)) if a != b))
    } else {
        false
    };
    if conflict {
        let error = Error::msg(
            "NVMe Identify",
            "ambiguous: conflicting namespace identity association",
        );
        storage_diagnostic(
            index,
            Some(bus),
            "StorageDeviceProtocolSpecificProperty CNS=3 association",
            None,
            started,
            Some(&error),
            "ok",
        );
        *namespace_descriptors = IdentifyOutcome::Failed(error);
    }
}

const NVME_IDENTIFY_SIZE: usize = 4096;
const PROTOCOL_START: usize = offset_of!(STORAGE_PROTOCOL_DATA_DESCRIPTOR, ProtocolSpecificData);

fn nvme_identify<T>(
    index: u32,
    cns: u32,
    deadline: Instant,
    parse: impl FnOnce(&[u8]) -> Result<Option<T>>,
) -> IdentifyOutcome<T> {
    let started = Instant::now();
    let data = bounded("NVMe Identify", deadline, move || {
        let handle = ioctl::open_device(&format!(r"\\.\PHYSICALDRIVE{index}"), GENERIC_READ.0)?;
        // STORAGE_PROPERTY_QUERY.AdditionalParameters starts at byte 8, not
        // sizeof(STORAGE_PROPERTY_QUERY): the SDK includes a one-byte tail.
        let mut query = [0u8; PROTOCOL_START + size_of::<STORAGE_PROTOCOL_SPECIFIC_DATA>()];
        let property = if cns == 1 {
            StorageAdapterProtocolSpecificProperty
        } else {
            StorageDeviceProtocolSpecificProperty
        };
        for (offset, value) in [
            (0, property.0 as u32),
            (4, PropertyStandardQuery.0 as u32),
            (8, ProtocolTypeNvme.0 as u32),
            (12, NVMeDataTypeIdentify.0 as u32),
            (16, cns), // Controller=1, namespace=0, namespace descriptor list=3.
            // Keep SubValue=0: use this physical-device property driver's default.
            // Never substitute a universal NSID 1; multi-namespace remains unverified.
            (24, size_of::<STORAGE_PROTOCOL_SPECIFIC_DATA>() as u32),
            (28, NVME_IDENTIFY_SIZE as u32),
        ] {
            query[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
        }
        ioctl::device_io_control(
            &handle,
            IOCTL_STORAGE_QUERY_PROPERTY,
            &query,
            8192, // Slack avoids the shared helper retrying a completely full reply.
        )
    });
    let returned = data.as_ref().ok().map(Vec::len);
    let result = data.and_then(|data| parse(nvme_payload(&data)?));
    storage_diagnostic(
        index,
        Some(BusTypeNvme.0 as u32),
        match cns {
            1 => "StorageAdapterProtocolSpecificProperty CNS=1 SubValue=0",
            3 => "StorageDeviceProtocolSpecificProperty CNS=3 SubValue=0",
            _ => "StorageDeviceProtocolSpecificProperty CNS=0 SubValue=0",
        },
        returned,
        started,
        result.as_ref().err(),
        if matches!(result, Ok(None)) {
            "empty"
        } else {
            "ok"
        },
    );
    match result {
        Ok(Some(value)) => IdentifyOutcome::Ok(value),
        Ok(None) => IdentifyOutcome::Empty,
        Err(error) => IdentifyOutcome::Failed(error),
    }
}

// The existing helper diagnostic channel stores Error-shaped records. The class
// explicitly distinguishes telemetry for success/empty/skip from actual failures.
fn storage_diagnostic(
    index: u32,
    bus: Option<u32>,
    request: &str,
    returned: Option<usize>,
    started: Instant,
    error: Option<&Error>,
    status: &str,
) {
    let class = match error {
        None if status == "empty" => "absent",
        None if status == "not-attempted" => "unsupported",
        None => status,
        Some(error) if error.detail.starts_with("implausible:") => "implausible",
        Some(error) if error.detail.starts_with("ambiguous:") => "ambiguous",
        Some(error) if error.detail.starts_with("placeholder:") => "placeholder",
        Some(error)
            if matches!(error.code, 121 | 258 | 1460)
                || error.detail.contains("timed out")
                || error.detail.starts_with("timeout:") =>
        {
            "timeout"
        }
        Some(error) if error.code == 5 => "access-denied",
        Some(error) if matches!(error.code, 1 | 50 | 87) => "unsupported",
        Some(error) if matches!(error.code, 21 | 1112 | 1167) => "absent",
        Some(error)
            if matches!(
                error.op,
                "storage descriptor" | "storage parser" | "ATA task file"
            ) && error.code == 0
                || matches!(error.op, "NVMe Identify" | "ATA Identify")
                    && error.code == 0
                    && (error.detail.starts_with("invalid ")
                        || error.detail.starts_with("protocol payload ")) =>
        {
            "malformed"
        }
        Some(_) => "failed",
    };
    let bus = bus.map_or_else(|| "unknown".into(), |bus| bus.to_string());
    let returned = returned.map_or_else(|| "unknown".into(), |size| size.to_string());
    super::record(Error {
        op: "storage query diagnostic",
        code: error.map_or(0, |error| error.code),
        detail: format!(
            "path=\\\\.\\PHYSICALDRIVE{index} BusType={bus} property={request} returned={returned} elapsed_ms={} class={class}{}",
            started.elapsed().as_millis(),
            error.map_or_else(String::new, |error| format!(" error={error}")),
        ),
    });
}

fn nvme_payload(data: &[u8]) -> Result<&[u8]> {
    let header_size = size_of::<STORAGE_PROTOCOL_DATA_DESCRIPTOR>() as u32;
    if dword(data, 0)? != header_size
        || dword(data, 4)? != header_size
        || dword(data, 8)? != ProtocolTypeNvme.0 as u32
        || dword(data, 12)? != NVMeDataTypeIdentify.0 as u32
    {
        return Err(Error::msg("NVMe Identify", "invalid protocol descriptor"));
    }
    let offset = dword(data, 24)? as usize;
    let length = dword(data, 28)? as usize;
    if offset < size_of::<STORAGE_PROTOCOL_SPECIFIC_DATA>() || length != NVME_IDENTIFY_SIZE {
        return Err(Error::msg(
            "NVMe Identify",
            "invalid protocol payload size or offset",
        ));
    }
    let start = PROTOCOL_START.checked_add(offset);
    start
        .and_then(|start| {
            start
                .checked_add(length)
                .and_then(|end| data.get(start..end))
        })
        .ok_or_else(|| Error::msg("NVMe Identify", "protocol payload exceeds returned buffer"))
}

fn parse_nvme_serial(payload: &[u8]) -> Result<Option<String>> {
    let serial = bytes::<20>(payload, 4)?;
    // Some devices NUL-pad instead of space-pad. Reject control/non-ASCII bytes
    // inside the serial rather than injecting them into the report's tree.
    let padding = |b: &u8| *b == 0 || *b == b' ';
    let start = serial
        .iter()
        .position(|b| !padding(b))
        .unwrap_or(serial.len());
    let end = serial
        .iter()
        .rposition(|b| !padding(b))
        .map_or(start, |i| i + 1);
    let serial = serial
        .get(start..end)
        .ok_or_else(|| Error::msg("NVMe Identify", "invalid trimmed serial bounds"))?;
    if serial.iter().any(|b| !(0x20..=0x7e).contains(b)) {
        return Err(Error::msg(
            "NVMe Identify",
            "invalid controller serial encoding",
        ));
    }
    if serial.is_empty() {
        return Ok(None);
    }
    if serial
        .first()
        .is_some_and(|first| serial.iter().all(|b| b == first))
    {
        return Err(Error::msg(
            "NVMe Identify",
            "implausible: repeated controller serial character",
        ));
    }
    let text: String = serial.iter().map(|&b| char::from(b)).collect();
    if matches!(
        text.to_ascii_uppercase().as_str(),
        "UNKNOWN"
            | "UNKNOWN SERIAL"
            | "NONE"
            | "N/A"
            | "NA"
            | "NOT SPECIFIED"
            | "NOT AVAILABLE"
            | "DEFAULT STRING"
            | "TO BE FILLED BY O.E.M."
    ) {
        return Err(Error::msg(
            "NVMe Identify",
            "placeholder: controller serial omitted",
        ));
    }
    Ok(Some(text))
}

fn namespace_identifier(value: &[u8], source: &'static str) -> Option<String> {
    if value.iter().all(|&b| b == 0) || value.iter().all(|&b| b == 0xFF) {
        super::record(Error::msg(
            source,
            if value.iter().all(|&b| b == 0) {
                "absent: zero identity"
            } else {
                "implausible: all-FF identity"
            },
        ));
        None
    } else {
        Some(value.iter().map(|b| format!("{b:02X}")).collect())
    }
}

fn parse_nvme_namespace(payload: &[u8]) -> Result<NvmeNamespace> {
    Ok(NvmeNamespace {
        eui64: namespace_identifier(&bytes::<8>(payload, 120)?, "NVMe CNS 0 EUI-64"),
        nguid: namespace_identifier(&bytes::<16>(payload, 104)?, "NVMe CNS 0 NGUID"),
    })
}

fn parse_nvme_namespace_descriptors(payload: &[u8]) -> Result<NvmeNamespaceDescriptors> {
    if payload.len() != NVME_IDENTIFY_SIZE {
        return Err(Error::msg(
            "NVMe Identify",
            "invalid namespace descriptor list size",
        ));
    }
    let mut ids = NvmeNamespaceDescriptors::default();
    let mut seen = [false; 5];
    let mut offset = 0;
    while offset < payload.len() {
        let header = bytes::<4>(payload, offset)?;
        let kind = header[0] as usize;
        let length = header[1] as usize;
        if length == 0 {
            if payload
                .get(offset..)
                .is_none_or(|tail| tail.iter().any(|&b| b != 0))
            {
                return Err(Error::msg(
                    "NVMe Identify",
                    "invalid namespace descriptor terminator",
                ));
            }
            break;
        }
        if kind == 0 || header[2..].iter().any(|&b| b != 0) {
            return Err(Error::msg(
                "NVMe Identify",
                "invalid namespace descriptor header",
            ));
        }
        let start = offset
            .checked_add(4)
            .ok_or_else(|| Error::msg("NVMe Identify", "invalid descriptor offset"))?;
        let end = start
            .checked_add(length)
            .ok_or_else(|| Error::msg("NVMe Identify", "invalid descriptor length"))?;
        let value = payload
            .get(start..end)
            .ok_or_else(|| Error::msg("NVMe Identify", "invalid truncated namespace identifier"))?;
        if let Some(seen_kind) = seen.get_mut(kind) {
            let expected = match kind {
                1 => 8,
                2 | 3 => 16,
                4 => 1,
                _ => {
                    return Err(Error::msg(
                        "NVMe Identify",
                        "invalid namespace identifier type",
                    ));
                }
            };
            if length != expected || *seen_kind {
                return Err(Error::msg(
                    "NVMe Identify",
                    "invalid namespace identifier length or duplicate type",
                ));
            }
            *seen_kind = true;
            match kind {
                1 => ids.eui64 = namespace_identifier(value, "NVMe CNS 3 EUI-64"),
                2 => ids.nguid = namespace_identifier(value, "NVMe CNS 3 NGUID"),
                3 => {
                    if let Some(hex) = namespace_identifier(value, "NVMe CNS 3 UUID") {
                        ids.uuid = Some(format!(
                            "{}-{}-{}-{}-{}",
                            &hex[..8],
                            &hex[8..12],
                            &hex[12..16],
                            &hex[16..20],
                            &hex[20..]
                        ));
                    }
                }
                4 => ids.csi = value.first().copied(),
                _ => {}
            }
        } else {
            super::record(Error::msg(
                "NVMe CNS 3",
                "unsupported: unknown bounded namespace descriptor type omitted",
            ));
        }
        // Unknown nonzero types are length-checked and skipped for future specs.
        offset = end;
    }
    Ok(ids)
}

/// Reads GPT/MBR identity through the shared device/IOCTL helpers.
pub fn physical_layout(index: u32) -> Result<Option<DiskLayout>> {
    physical_layout_until(index, Instant::now() + DISK_BUDGET)
}

fn physical_layout_until(index: u32, deadline: Instant) -> Result<Option<DiskLayout>> {
    bounded("IOCTL_DISK_GET_DRIVE_LAYOUT_EX", deadline, move || {
        // C# parity: Services/Win32/StorageDeviceIdQuery.cs:354-363.
        let handle = ioctl::open_device(&format!(r"\\.\PHYSICALDRIVE{index}"), GENERIC_READ.0)?;
        parse_layout(&ioctl::device_io_control(
            &handle,
            IOCTL_DISK_GET_DRIVE_LAYOUT_EX,
            &[],
            4096,
        )?)
    })
}

fn bytes<const N: usize>(data: &[u8], offset: usize) -> Result<[u8; N]> {
    let end = offset
        .checked_add(N)
        .ok_or_else(|| Error::msg("storage parser", "offset overflow"))?;
    data.get(offset..end)
        .and_then(|b| b.try_into().ok())
        .ok_or_else(|| Error::msg("storage parser", "truncated buffer"))
}

fn dword(data: &[u8], offset: usize) -> Result<u32> {
    Ok(u32::from_le_bytes(bytes(data, offset)?))
}
fn word(data: &[u8], offset: usize) -> Result<u16> {
    Ok(u16::from_le_bytes(bytes(data, offset)?))
}

fn parse_descriptor(data: &[u8]) -> Result<Option<Identifier>> {
    const HEADER: usize = offset_of!(STORAGE_DEVICE_ID_DESCRIPTOR, Identifiers);
    const PAYLOAD: usize = offset_of!(STORAGE_IDENTIFIER, Identifier);
    let size = dword(data, offset_of!(STORAGE_DEVICE_ID_DESCRIPTOR, Size))? as usize;
    if size < HEADER || size > data.len() {
        return Err(Error::msg("storage descriptor", "invalid descriptor size"));
    }
    let data = &data[..size];
    let count = dword(
        data,
        offset_of!(STORAGE_DEVICE_ID_DESCRIPTOR, NumberOfIdentifiers),
    )? as usize;
    if count > (size - HEADER) / PAYLOAD {
        return Err(Error::msg(
            "storage descriptor",
            "identifier count exceeds buffer",
        ));
    }
    let mut offset = HEADER;
    let mut best = None;
    let mut best_rank = 3;
    for i in 0..count {
        let header = data
            .get(offset..offset + PAYLOAD)
            .ok_or_else(|| Error::msg("storage descriptor", "truncated identifier header"))?;
        let code = dword(header, offset_of!(STORAGE_IDENTIFIER, CodeSet))?;
        let kind = dword(header, offset_of!(STORAGE_IDENTIFIER, Type))?;
        let length = word(header, offset_of!(STORAGE_IDENTIFIER, IdentifierSize))? as usize;
        let next = word(header, offset_of!(STORAGE_IDENTIFIER, NextOffset))? as usize;
        let end = offset + PAYLOAD + length;
        let value = data.get(offset + PAYLOAD..end).ok_or_else(|| {
            Error::msg("storage descriptor", "identifier size exceeds descriptor")
        })?;
        // C# parity: Services/Win32/StorageDeviceIdQuery.cs:519-540. Association
        // does not filter selection: first ASCII, then nonzero binary 8/16, first.
        let rank = if code == StorageIdCodeSetAscii.0 as u32
            && (kind == StorageIdTypeScsiNameString.0 as u32
                || kind == StorageIdTypeVendorId.0 as u32
                || length > 0)
        {
            0
        } else if code == StorageIdCodeSetBinary.0 as u32
            && matches!(length, 8 | 16)
            && value.iter().any(|&b| b != 0)
        {
            1
        } else {
            2
        };
        if rank < best_rank {
            best = Some((code, value));
            best_rank = rank;
        }
        // Real drivers set the last NextOffset to its aligned size, which may point
        // at or past Size. C# parity: Services/Win32/StorageDeviceIdQuery.cs:541-545
        // (stop at count or a zero NextOffset and keep the best identifier so far).
        if i + 1 == count || next == 0 {
            break;
        }
        if next < PAYLOAD + length || offset + next > size {
            return Err(Error::msg(
                "storage descriptor",
                "invalid next identifier offset",
            ));
        }
        offset += next;
    }
    Ok(best.map(|(code, value)| {
        // C# parity: Services/Win32/StorageDeviceIdQuery.cs:306-311,595-609.
        // ASCII decoding replaces high bytes with '?' and trims only trailing NUL.
        let printable =
            !value.is_empty() && value.iter().all(|&b| b == 0 || (32..=126).contains(&b));
        let decoded = if code == StorageIdCodeSetAscii.0 as u32 || printable {
            value
                .iter()
                .map(|&b| if b.is_ascii() { char::from(b) } else { '?' })
                .collect::<String>()
                .trim_end_matches('\0')
                .to_owned()
        } else {
            String::new()
        };
        Identifier {
            hex: colon_hex(value),
            decoded,
        }
    }))
}

fn parse_identifier_list(data: &[u8]) -> Result<Vec<StorageIdentifier>> {
    const HEADER: usize = offset_of!(STORAGE_DEVICE_ID_DESCRIPTOR, Identifiers);
    const PAYLOAD: usize = offset_of!(STORAGE_IDENTIFIER, Identifier);
    if dword(data, offset_of!(STORAGE_DEVICE_ID_DESCRIPTOR, Version))? as usize
        != size_of::<STORAGE_DEVICE_ID_DESCRIPTOR>()
    {
        return Err(Error::msg(
            "storage descriptor",
            "invalid descriptor list version",
        ));
    }
    let size = dword(data, offset_of!(STORAGE_DEVICE_ID_DESCRIPTOR, Size))? as usize;
    if size < HEADER || size > data.len() {
        return Err(Error::msg(
            "storage descriptor",
            "invalid descriptor list size",
        ));
    }
    let data = data
        .get(..size)
        .ok_or_else(|| Error::msg("storage descriptor", "invalid descriptor list bounds"))?;
    let count = dword(
        data,
        offset_of!(STORAGE_DEVICE_ID_DESCRIPTOR, NumberOfIdentifiers),
    )? as usize;
    if count > 256 || count > (size - HEADER) / PAYLOAD || count == 0 && size != HEADER {
        return Err(Error::msg(
            "storage descriptor",
            "identifier count inconsistent with buffer",
        ));
    }
    let mut ids = Vec::with_capacity(count);
    let mut offset = HEADER;
    for i in 0..count {
        let payload_start = offset.checked_add(PAYLOAD).ok_or_else(|| {
            Error::msg("storage descriptor", "invalid identifier offset overflow")
        })?;
        let header = data
            .get(offset..payload_start)
            .ok_or_else(|| Error::msg("storage descriptor", "truncated identifier list header"))?;
        let length = word(header, offset_of!(STORAGE_IDENTIFIER, IdentifierSize))? as usize;
        let next = word(header, offset_of!(STORAGE_IDENTIFIER, NextOffset))? as usize;
        let span = PAYLOAD
            .checked_add(length)
            .ok_or_else(|| Error::msg("storage descriptor", "invalid identifier span overflow"))?;
        let end = payload_start.checked_add(length).ok_or_else(|| {
            Error::msg("storage descriptor", "invalid identifier length overflow")
        })?;
        let value = data.get(payload_start..end).ok_or_else(|| {
            Error::msg(
                "storage descriptor",
                "identifier size exceeds descriptor list",
            )
        })?;
        if i + 1 < count {
            let next_offset = offset
                .checked_add(next)
                .ok_or_else(|| Error::msg("storage descriptor", "invalid next offset overflow"))?;
            if next < span
                || next > span.next_multiple_of(8)
                || next_offset.saturating_add(PAYLOAD) > size
                || data
                    .get(end..next_offset)
                    .is_none_or(|padding| padding.iter().any(|&b| b != 0))
            {
                return Err(Error::msg(
                    "storage descriptor",
                    "invalid next identifier list offset or count",
                ));
            }
        } else {
            // Drivers may put the final aligned record size in NextOffset. Accept
            // at most 7 bytes of terminal alignment slack, never another record.
            let padding = data
                .get(end..)
                .ok_or_else(|| Error::msg("storage descriptor", "invalid padding bounds"))?;
            if padding.len() >= 8
                || padding.iter().any(|&b| b != 0)
                || next != 0
                    && (next < span
                        || next > span.next_multiple_of(8)
                        || offset.saturating_add(next) < size)
            {
                return Err(Error::msg(
                    "storage descriptor",
                    "invalid terminal identifier offset or count",
                ));
            }
        }
        ids.push(StorageIdentifier {
            code_set: dword(header, offset_of!(STORAGE_IDENTIFIER, CodeSet))?,
            identifier_type: dword(header, offset_of!(STORAGE_IDENTIFIER, Type))?,
            association: dword(header, offset_of!(STORAGE_IDENTIFIER, Association))?,
            value: value.to_vec(),
        });
        offset = offset
            .checked_add(next)
            .ok_or_else(|| Error::msg("storage descriptor", "invalid next offset overflow"))?;
    }
    Ok(ids)
}

fn parse_layout(data: &[u8]) -> Result<Option<DiskLayout>> {
    const HEADER: usize = offset_of!(DRIVE_LAYOUT_INFORMATION_EX, PartitionEntry);
    const UNION: usize = offset_of!(DRIVE_LAYOUT_INFORMATION_EX, Anonymous);
    if data.len() < HEADER {
        return Err(Error::msg("drive layout", "truncated layout header"));
    }
    let style = dword(
        data,
        offset_of!(DRIVE_LAYOUT_INFORMATION_EX, PartitionStyle),
    )?;
    // C# parity: Services/Win32/StorageDeviceIdQuery.cs:460-469 (RAW has no IDs).
    if style != PARTITION_STYLE_GPT.0 as u32 && style != PARTITION_STYLE_MBR.0 as u32 {
        return Ok(None);
    }
    let count = dword(
        data,
        offset_of!(DRIVE_LAYOUT_INFORMATION_EX, PartitionCount),
    )? as usize;
    if count > (data.len() - HEADER) / size_of::<PARTITION_INFORMATION_EX>() {
        return Err(Error::msg(
            "drive layout",
            "partition count exceeds returned buffer",
        ));
    }
    let is_gpt = style == PARTITION_STYLE_GPT.0 as u32;
    let disk_id = if is_gpt {
        guid_text(bytes(
            data,
            UNION + offset_of!(DRIVE_LAYOUT_INFORMATION_GPT, DiskId),
        )?)
    } else {
        format!(
            "0x{:08X}",
            dword(
                data,
                UNION + offset_of!(DRIVE_LAYOUT_INFORMATION_MBR, Signature)
            )?
        )
    };
    let mut partition_guids = Vec::new();
    if is_gpt {
        for i in 0..count {
            let offset = HEADER + i * size_of::<PARTITION_INFORMATION_EX>();
            // C# parity: Services/Win32/StorageDeviceIdQuery.cs:438-457.
            if dword(
                data,
                offset + offset_of!(PARTITION_INFORMATION_EX, PartitionStyle),
            )? == PARTITION_STYLE_GPT.0 as u32
            {
                let guid = bytes(
                    data,
                    offset
                        + offset_of!(PARTITION_INFORMATION_EX, Anonymous)
                        + offset_of!(PARTITION_INFORMATION_GPT, PartitionId),
                )?;
                if guid != [0; 16] {
                    partition_guids.push(guid_text(guid));
                }
            }
        }
    }
    Ok(Some(DiskLayout {
        is_gpt,
        disk_id,
        partition_guids,
    }))
}

fn single_extent_disk(data: &[u8]) -> Result<Option<u32>> {
    const HEADER: usize = offset_of!(VOLUME_DISK_EXTENTS, Extents);
    let count = dword(data, offset_of!(VOLUME_DISK_EXTENTS, NumberOfDiskExtents))? as usize;
    if data.len() < HEADER || count > (data.len() - HEADER) / size_of::<DISK_EXTENT>() {
        return Err(Error::msg("volume extents", "extent count exceeds buffer"));
    }
    let mut disk = None;
    for i in 0..count {
        let number = dword(
            data,
            HEADER + i * size_of::<DISK_EXTENT>() + offset_of!(DISK_EXTENT, DiskNumber),
        )?;
        if disk.is_some_and(|old| old != number) {
            return Ok(None);
        }
        disk = Some(number);
    }
    Ok(disk)
}

/// Formats bytes as the C# uppercase, colon-separated hexadecimal representation.
pub fn colon_hex(bytes: &[u8]) -> String {
    bytes
        .iter()
        .map(|b| format!("{b:02X}"))
        .collect::<Vec<_>>()
        .join(":")
}

fn guid_text(bytes: [u8; 16]) -> String {
    // C# parity: Hardware/DiskDriveInfo.cs:185-191 (Guid D, uppercase).
    format!(
        "{:08X}-{:04X}-{:04X}-{:02X}{:02X}-{:02X}{:02X}{:02X}{:02X}{:02X}{:02X}",
        u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]),
        u16::from_le_bytes([bytes[4], bytes[5]]),
        u16::from_le_bytes([bytes[6], bytes[7]]),
        bytes[8],
        bytes[9],
        bytes[10],
        bytes[11],
        bytes[12],
        bytes[13],
        bytes[14],
        bytes[15]
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    #[test]
    #[ignore = "shared helper diagnostic queue; run alone with --ignored --test-threads=1"]
    fn failsafe_storage_diagnostics_are_value_free_and_classified() {
        let _ = super::super::take_recorded();
        let _ = physical_ata_identity(87654, 7).unwrap();
        let mut data = ata_fixture();
        data[216..224].fill(0xFF);
        assert!(matches!(
            parse_ata_identity(&data).unwrap().wwn,
            IdentifyOutcome::Empty
        ));
        let mut fixed = [0; NVME_IDENTIFY_SIZE];
        fixed[104..120].fill(0xFF);
        let _ = parse_nvme_namespace(&fixed).unwrap();
        let mut invalid = [0; NVME_IDENTIFY_SIZE];
        invalid[..2].copy_from_slice(&[3, 15]);
        let error = parse_nvme_namespace_descriptors(&invalid).unwrap_err();
        storage_diagnostic(
            87654,
            Some(17),
            "CNS 3 fixture",
            Some(invalid.len()),
            Instant::now(),
            Some(&error),
            "ok",
        );
        let fixed = IdentifyOutcome::Ok(NvmeNamespace {
            eui64: Some("002538C86B1479A2".into()),
            nguid: None,
        });
        let mut ids = IdentifyOutcome::Ok(NvmeNamespaceDescriptors {
            eui64: Some("002538C86B1479B3".into()),
            ..Default::default()
        });
        validate_namespace_association(87654, 17, Instant::now(), &fixed, &mut ids);
        let records = super::super::take_recorded();
        assert!(
            records
                .iter()
                .any(|e| e.detail.contains("class=unsupported"))
        );
        assert!(records.iter().any(|e| e.detail.contains("implausible")));
        assert!(records.iter().any(|e| e.detail.contains("class=malformed")));
        assert!(records.iter().any(|e| e.detail.contains("class=ambiguous")));
        assert!(records.iter().all(|e| !e.detail.contains("002538C8")));
    }

    #[test]
    fn disk_jobs_over_four_keep_order_limit_and_isolate_panics() {
        let indices = [9, 3, 7, 2, 5, 1, 4];
        let active = AtomicUsize::new(0);
        let peak = AtomicUsize::new(0);
        let result = disk_jobs(&indices, &|index, deadline| {
            let count = active.fetch_add(1, Ordering::SeqCst) + 1;
            peak.fetch_max(count, Ordering::SeqCst);
            assert!(deadline.saturating_duration_since(Instant::now()) <= DISK_BUDGET);
            thread::sleep(Duration::from_millis(15));
            active.fetch_sub(1, Ordering::SeqCst);
            if index == 2 {
                panic!("fabricated job panic");
            }
            PhysicalQueries {
                nvme: Ok(NvmeIdentity {
                    controller_serial: IdentifyOutcome::Ok(format!("disk-{index}")),
                    namespace: IdentifyOutcome::Empty,
                    namespace_descriptors: IdentifyOutcome::Empty,
                }),
                ata: None,
                identifiers: Ok(StorageIdentifiers {
                    selected: None,
                    descriptors: IdentifyOutcome::Empty,
                }),
                layout: Ok(None),
            }
        });
        assert_eq!(result.len(), indices.len());
        assert!(peak.load(Ordering::SeqCst) <= DISK_JOBS);
        for (index, result) in indices.into_iter().zip(result) {
            if index == 2 {
                assert!(result.nvme.is_err());
                assert!(result.layout.is_err());
            } else {
                assert!(
                    matches!(result.nvme.unwrap().controller_serial, IdentifyOutcome::Ok(s) if s == format!("disk-{index}"))
                );
            }
        }
        let slots = AtomicUsize::new(0);
        for _ in 0..IO_JOBS {
            assert!(reserve_io_worker(&slots));
        }
        assert!(!reserve_io_worker(&slots));
        assert_eq!(slots.load(Ordering::SeqCst), IO_JOBS);
        slots.store(usize::MAX, Ordering::SeqCst);
        assert!(!reserve_io_worker(&slots));
    }

    #[test]
    fn ata_exclusion_survives_timeout_until_worker_returns() {
        let (release_tx, release_rx) = mpsc::channel();
        let (acquired_tx, acquired_rx) = mpsc::channel();
        let (finished_tx, finished_rx) = mpsc::channel();
        let error = bounded(
            "ATA exclusion fixture",
            Instant::now() + Duration::from_millis(100),
            move || {
                let guard = AtaWorker::acquire(87654)?;
                let _ = acquired_tx.send(());
                let _ = release_rx.recv_timeout(Duration::from_secs(5));
                drop(guard);
                let _ = finished_tx.send(());
                Ok(())
            },
        )
        .unwrap_err();
        acquired_rx.recv_timeout(Duration::from_secs(1)).unwrap();
        assert!(error.detail.contains("timed out"));
        assert!(AtaWorker::acquire(87654).is_err());
        assert!(AtaWorker::acquire(87655).is_ok());
        release_tx.send(()).unwrap();
        finished_rx.recv_timeout(Duration::from_secs(1)).unwrap();
        assert!(AtaWorker::acquire(87654).is_ok());
    }

    #[test]
    fn namespace_sentinels_conflicts_and_partial_success() {
        let mut fixed = [0; NVME_IDENTIFY_SIZE];
        fixed[120..128].copy_from_slice(&[0x00, 0x25, 0x38, 0xC8, 0x6B, 0x14, 0x79, 0xA2]);
        fixed[104..120].fill(0xFF);
        let namespace = parse_nvme_namespace(&fixed).unwrap();
        assert!(namespace.nguid.is_none());
        assert_eq!(namespace.eui64.as_deref(), Some("002538C86B1479A2"));
        let mut list = [0; NVME_IDENTIFY_SIZE];
        list[..2].copy_from_slice(&[1, 8]);
        list[4..12].copy_from_slice(&fixed[120..128]);
        list[12..14].copy_from_slice(&[3, 16]);
        list[16..32].fill(0xFF);
        let ids = parse_nvme_namespace_descriptors(&list).unwrap();
        assert!(ids.uuid.is_none());
        let namespace = IdentifyOutcome::Ok(namespace);
        let mut descriptor = IdentifyOutcome::Ok(ids);
        validate_namespace_association(87654, 17, Instant::now(), &namespace, &mut descriptor);
        assert!(matches!(descriptor, IdentifyOutcome::Ok(_)));
        list[11] ^= 1;
        descriptor = IdentifyOutcome::Ok(parse_nvme_namespace_descriptors(&list).unwrap());
        validate_namespace_association(87654, 17, Instant::now(), &namespace, &mut descriptor);
        assert!(
            matches!(descriptor, IdentifyOutcome::Failed(ref e) if e.detail.contains("ambiguous") && !e.detail.contains("002538"))
        );
        assert!(matches!(namespace, IdentifyOutcome::Ok(ref n) if n.eui64.is_some()));
        descriptor = IdentifyOutcome::Ok(parse_nvme_namespace_descriptors(&list).unwrap());
        validate_namespace_association(
            87654,
            17,
            Instant::now(),
            &IdentifyOutcome::Failed(Error::msg("NVMe Identify", "timeout: fabricated")),
            &mut descriptor,
        );
        assert!(matches!(descriptor, IdentifyOutcome::Ok(_))); // CNS 3 can backfill.
        let mut controller = [b' '; 64];
        controller[4..24].fill(b'A');
        assert!(
            parse_nvme_serial(&controller)
                .unwrap_err()
                .detail
                .contains("implausible")
        );
        assert!(ata_string(&[], usize::MAX, usize::MAX, true).is_err());
        assert!(ata_string(&[], 10, 20, true).is_err());
    }

    #[test]
    fn unsupported_disk_buses_never_issue_ata_or_emit_fields() {
        for bus in [0, 7, 8, 16, 17] {
            // Unknown, USB, RAID, Storage Spaces, NVMe.
            let result = physical_ata_identity(87654, bus).unwrap();
            assert!(matches!(result.serial, IdentifyOutcome::NotAttempted { bus: b } if b == bus));
            assert!(matches!(result.model, IdentifyOutcome::NotAttempted { .. }));
            assert!(matches!(result.wwn, IdentifyOutcome::NotAttempted { .. }));
        }
    }

    #[test]
    fn deadline_retains_running_worker_and_refuses_later_work() {
        // A real healthy disk cannot exercise an indefinitely blocked driver.
        // This owned allocation stands in for that worker's handles/buffers.
        struct OwnedQuery(mpsc::Sender<()>);
        impl Drop for OwnedQuery {
            fn drop(&mut self) {
                let _ = self.0.send(());
            }
        }
        let (started_tx, started_rx) = mpsc::channel();
        let (release_tx, release_rx) = mpsc::channel();
        let (dropped_tx, dropped_rx) = mpsc::channel();
        let owned = OwnedQuery(dropped_tx);
        let deadline = Instant::now() + Duration::from_millis(500);
        let error = bounded("deadline fixture", deadline, move || {
            let _owned = owned;
            let _ = started_tx.send(());
            let _ = release_rx.recv_timeout(Duration::from_secs(5));
            Ok(())
        })
        .unwrap_err();
        started_rx.recv_timeout(Duration::from_secs(1)).unwrap();
        assert!(error.detail.contains("timed out"));
        assert!(
            dropped_rx.try_recv().is_err(),
            "worker still owns the query"
        );
        let ran = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let ran_worker = std::sync::Arc::clone(&ran);
        assert!(
            bounded("expired fixture", deadline, move || {
                ran_worker.store(true, Ordering::Relaxed);
                Ok(())
            })
            .is_err()
        );
        assert!(!ran.load(Ordering::Relaxed));
        // A successful independent query is still possible after another caller
        // abandoned its wait; it cannot drop the blocked query's allocation.
        assert_eq!(
            bounded(
                "healthy fixture",
                Instant::now() + Duration::from_secs(1),
                || Ok(17)
            )
            .unwrap(),
            17
        );
        assert!(dropped_rx.try_recv().is_err());
        release_tx.send(()).unwrap();
        dropped_rx.recv_timeout(Duration::from_secs(1)).unwrap();
    }

    #[test]
    fn full_storage_identifier_list_checks_count_offsets_and_metadata() {
        let fixture: Value =
            serde_json::from_str(include_str!("../../tests/fixtures/wp-01/storage.json")).unwrap();
        for case in fixture["descriptors"].as_array().unwrap() {
            let data = fixture_bytes(case);
            let rejects = case["error"] == true
                || matches!(
                    case["name"].as_str(),
                    Some(
                        "count-chain-short-zero-next-keeps-best"
                            | "count-reached-last-next-nonzero"
                    )
                );
            assert_eq!(
                parse_identifier_list(&data).is_err(),
                rejects,
                "{}",
                case["name"]
            );
            for end in 0..data.len() {
                assert!(
                    parse_identifier_list(&data[..end]).is_err(),
                    "{} prefix {end}",
                    case["name"]
                );
            }
        }
        let data = fixture_bytes(&fixture["descriptors"][0]);
        let ids = parse_identifier_list(&data).unwrap();
        assert_eq!(ids.len(), 3);
        assert_eq!(
            (ids[0].code_set, ids[0].identifier_type, ids[0].association),
            (1, 2, 0)
        );
        assert_eq!(
            (ids[1].code_set, ids[1].identifier_type, ids[1].association),
            (2, 8, 1)
        );
        assert_eq!(ids[1].value, b"NVME-S9Z3NX0W713842\0");
        let mut invalid = data.clone();
        // A count that ends before another real descriptor cannot silently truncate.
        invalid[8..12].copy_from_slice(&2u32.to_le_bytes());
        assert!(parse_identifier_list(&invalid).is_err());
        // A next offset cannot skip a nonzero hidden record or arbitrary slack.
        let mut hidden = data.clone();
        hidden[22..24].copy_from_slice(&40u16.to_le_bytes());
        assert!(parse_identifier_list(&hidden).is_err());
        let mut unknown_version = data.clone();
        unknown_version[..4].copy_from_slice(&0u32.to_le_bytes());
        assert!(parse_identifier_list(&unknown_version).is_err());
        // Full-list failure cannot erase the legacy selector's selected value.
        assert_eq!(
            parse_descriptor(&unknown_version).unwrap(),
            parse_descriptor(&data).unwrap()
        );
        let mut terminal = fixture_bytes(&fixture["descriptors"][15]);
        for next in [1u16, 35, 41, u16::MAX] {
            terminal[22..24].copy_from_slice(&next.to_le_bytes());
            assert!(parse_identifier_list(&terminal).is_err());
        }
        // A final aligned size may extend just beyond Size; no next entry is read.
        terminal[4..8].copy_from_slice(&48u32.to_le_bytes());
        terminal[22..24].copy_from_slice(&40u16.to_le_bytes());
        assert_eq!(parse_identifier_list(&terminal[..48]).unwrap().len(), 1);
    }

    #[test]
    fn nvme_namespace_descriptor_types_lengths_uuid_order_and_terminator() {
        let mut data = vec![0; NVME_IDENTIFY_SIZE];
        let mut offset = 0;
        for (kind, value) in [
            (1, vec![0x00, 0x25, 0x38, 0xC8, 0x6B, 0x14, 0x79, 0xA2]),
            (
                2,
                vec![
                    0x6E, 0x84, 0x93, 0xF1, 0x72, 0xB5, 0x4C, 0xA0, 0x91, 0xD8, 0xE2, 0x60, 0x3F,
                    0x7A, 0x4B, 0x65,
                ],
            ),
            (
                3,
                vec![
                    0xD1, 0x97, 0xA2, 0x34, 0x51, 0xC8, 0x4E, 0x62, 0x9B, 0x17, 0xA0, 0x64, 0x32,
                    0xDF, 0x87, 0x90,
                ],
            ),
            (4, vec![0]),
            (0x80, vec![0x12, 0x34]), // Bounded future type, not a known identity.
        ] {
            data[offset] = kind;
            data[offset + 1] = value.len() as u8;
            data[offset + 4..offset + 4 + value.len()].copy_from_slice(&value);
            offset += 4 + value.len();
        }
        let ids = parse_nvme_namespace_descriptors(&data).unwrap();
        assert_eq!(ids.eui64.as_deref(), Some("002538C86B1479A2"));
        assert_eq!(
            ids.nguid.as_deref(),
            Some("6E8493F172B54CA091D8E2603F7A4B65")
        );
        assert_eq!(
            ids.uuid.as_deref(),
            Some("D197A234-51C8-4E62-9B17-A06432DF8790")
        );
        assert_eq!(ids.csi, Some(0));
        for end in 0..data.len() {
            assert!(parse_nvme_namespace_descriptors(&data[..end]).is_err());
        }
        for (at, value) in [
            (1, 7),
            (13, 15),
            (33, 15),
            (53, 2),
            (2, 1),
            (0, 0),
            (offset, 1),
            (offset + 4, 1),
        ] {
            let mut invalid = data.clone();
            invalid[at] = value;
            assert!(
                parse_nvme_namespace_descriptors(&invalid).is_err(),
                "offset {at}"
            );
        }
        let mut duplicate = data.clone();
        duplicate[offset..offset + 12].copy_from_slice(&data[..12]);
        assert!(parse_nvme_namespace_descriptors(&duplicate).is_err());
        let mut truncated_value = vec![0; NVME_IDENTIFY_SIZE];
        // A run of 15-byte future descriptors leaves only seven payload bytes.
        for at in (0..4092).step_by(19) {
            truncated_value[at] = 0x80;
            truncated_value[at + 1] = 15;
        }
        truncated_value[4085] = 3;
        truncated_value[4086] = 16;
        assert!(parse_nvme_namespace_descriptors(&truncated_value).is_err());
        assert_eq!(
            parse_nvme_namespace_descriptors(&vec![0; NVME_IDENTIFY_SIZE]).unwrap(),
            NvmeNamespaceDescriptors::default()
        );
        let mut zero_uuid = vec![0; NVME_IDENTIFY_SIZE];
        zero_uuid[..2].copy_from_slice(&[3, 16]);
        assert!(
            parse_nvme_namespace_descriptors(&zero_uuid)
                .unwrap()
                .uuid
                .is_none()
        );
    }

    fn ata_fixture() -> [u8; ATA_IDENTIFY_SIZE] {
        let mut data = [0; ATA_IDENTIFY_SIZE];
        for (start, length, text) in [
            (10, 20, "S6PUNF0R812345X"),
            (23, 8, "SVT02B6Q"),
            (27, 40, "Samsung SSD 870 EVO 1TB"),
        ] {
            let field = &mut data[start * 2..start * 2 + length];
            field.fill(b' ');
            field[..text.len()].copy_from_slice(text.as_bytes());
            for pair in field.as_chunks_mut::<2>().0 {
                pair.swap(0, 1);
            }
        }
        for (index, value) in [
            (84, 0x4100u16),
            (87, 0x4100),
            (108, 0x5002),
            (109, 0x538E),
            (110, 0xA1B2),
            (111, 0xC3D4),
        ] {
            data[index * 2..index * 2 + 2].copy_from_slice(&value.to_le_bytes());
        }
        data
    }

    #[test]
    fn ata_identify_word_order_validity_placeholders_and_partial_fields() {
        let mut data = ata_fixture();
        let identity = parse_ata_identity(&data).expect("fabricated Identify block");
        assert!(matches!(identity.serial, IdentifyOutcome::Ok(ref s) if s == "S6PUNF0R812345X"));
        assert!(
            matches!(identity.model, IdentifyOutcome::Ok(ref s) if s == "Samsung SSD 870 EVO 1TB")
        );
        assert!(matches!(identity.firmware, IdentifyOutcome::Ok(ref s) if s == "SVT02B6Q"));
        assert!(matches!(
            identity.wwn,
            IdentifyOutcome::Ok(0x5002_538E_A1B2_C3D4)
        ));
        for end in 0..ATA_IDENTIFY_SIZE {
            assert!(parse_ata_identity(&data[..end]).is_err());
        }
        for index in [84, 87] {
            for value in [0u16, 0x4000, 0x0100, 0xC100] {
                let mut invalid = data;
                invalid[index * 2..index * 2 + 2].copy_from_slice(&value.to_le_bytes());
                assert!(matches!(
                    parse_ata_identity(&invalid).unwrap().wwn,
                    IdentifyOutcome::Empty
                ));
            }
        }
        for sentinel in [0, 0xFF] {
            let mut invalid = data;
            invalid[216..224].fill(sentinel);
            assert!(matches!(
                parse_ata_identity(&invalid).unwrap().wwn,
                IdentifyOutcome::Empty
            ));
        }
        for serial in [
            "",
            "UNKNOWN",
            "Default String",
            "00000000000000000000",
            "FFFFFFFFFFFFFFFFFFFF",
            "N/A",
        ] {
            let mut invalid = data;
            invalid[20..40].fill(b' ');
            invalid[20..20 + serial.len()].copy_from_slice(serial.as_bytes());
            for pair in invalid[20..40].as_chunks_mut::<2>().0 {
                pair.swap(0, 1);
            }
            let identity = parse_ata_identity(&invalid).unwrap();
            assert!(matches!(identity.serial, IdentifyOutcome::Empty));
            assert!(matches!(identity.wwn, IdentifyOutcome::Ok(_)));
        }
        // A corrupt serial cannot erase the independently valid model/WWN.
        data[21] = 0x80;
        let identity = parse_ata_identity(&data).unwrap();
        assert!(
            matches!(identity.serial, IdentifyOutcome::Failed(ref e) if !e.detail.contains("S6PU"))
        );
        assert!(matches!(identity.model, IdentifyOutcome::Ok(_)));
        assert!(matches!(identity.wwn, IdentifyOutcome::Ok(_)));
        data = ata_fixture();
        data[510] = 0xA5;
        data[511] = 0u8.wrapping_sub(data.iter().fold(0u8, |sum, b| sum.wrapping_add(*b)));
        assert!(parse_ata_identity(&data).is_ok());
        data[511] = data[511].wrapping_add(1);
        assert!(parse_ata_identity(&data).is_err());
    }

    #[test]
    fn ata_reply_envelopes_and_task_file_status_are_checked() {
        // x64 ntddscsi.h ABI; protect the local layout against accidental changes.
        assert_eq!(size_of::<AtaPassThrough>(), 48);
        assert_eq!(offset_of!(AtaPassThrough, data_offset), 24);
        assert_eq!(offset_of!(AtaPassThrough, current_task_file), 40);
        assert_eq!(ATA_PASS_THROUGH_IOCTL, 0x4D02C);
        let header = size_of::<AtaPassThrough>();
        let mut reply = ata_passthrough_query();
        assert_eq!(reply[46], 0xEC);
        assert_eq!(word(&reply, 2).unwrap(), 3); // DATA_IN only, never DATA_OUT.
        reply[46] = 0x50;
        reply.extend_from_slice(&ata_fixture());
        assert_eq!(ata_passthrough_payload(&reply).unwrap(), ata_fixture());
        for status in [0x00, 0x51, 0x70, 0xD0, 0x58] {
            let mut invalid = reply.clone();
            invalid[46] = status;
            assert!(ata_passthrough_payload(&invalid).is_err());
        }
        for (offset, bytes) in [
            (0, 47u16.to_le_bytes().to_vec()),
            (8, 511u32.to_le_bytes().to_vec()),
            (24, 40usize.to_le_bytes().to_vec()),
        ] {
            let mut invalid = reply.clone();
            invalid[offset..offset + bytes.len()].copy_from_slice(&bytes);
            assert!(ata_passthrough_payload(&invalid).is_err());
        }
        for end in 0..reply.len() {
            assert!(ata_passthrough_payload(&reply[..end]).is_err());
        }
        let mut protocol = vec![0; PROTOCOL_START + size_of::<STORAGE_PROTOCOL_SPECIFIC_DATA>()];
        for (offset, value) in [
            (0, header as u32),
            (4, header as u32),
            (8, ProtocolTypeAta.0 as u32),
            (12, AtaDataTypeIdentify.0 as u32),
            (24, 40),
            (28, 512),
        ] {
            protocol[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
        }
        protocol.extend_from_slice(&ata_fixture());
        assert_eq!(ata_protocol_payload(&protocol).unwrap(), ata_fixture());
        for end in 0..protocol.len() {
            assert!(ata_protocol_payload(&protocol[..end]).is_err());
        }
        for (offset, value) in [
            (0, 0),
            (4, 0),
            (8, ProtocolTypeNvme.0 as u32),
            (12, 0),
            (24, 0),
            (24, u32::MAX),
            (28, 511),
            (28, 513),
        ] {
            let mut invalid = protocol.clone();
            invalid[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
            assert!(ata_protocol_payload(&invalid).is_err());
        }
        let skipped = physical_ata_identity(9999, BusTypeNvme.0 as u32).unwrap();
        assert!(matches!(
            skipped.serial,
            IdentifyOutcome::NotAttempted { .. }
        ));
    }

    fn fixture_bytes(case: &Value) -> Vec<u8> {
        let hex = case["hex"].as_str().expect("fixture hex");
        (0..hex.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).expect("hex byte"))
            .collect()
    }

    #[test]
    fn storage_descriptor_fixtures() {
        let nvme: Value =
            serde_json::from_str(include_str!("../../tests/fixtures/wp-01/nvme.json"))
                .expect("NVMe fixture JSON");
        assert_eq!(PROTOCOL_START, 8);
        assert_eq!(offset_of!(STORAGE_PROPERTY_QUERY, AdditionalParameters), 8);
        assert_eq!(size_of::<STORAGE_PROTOCOL_SPECIFIC_DATA>(), 40);
        for case in nvme["cases"].as_array().expect("NVMe cases") {
            let mut data = vec![0; case["length"].as_u64().expect("length") as usize];
            for segment in case["segments"].as_array().expect("segments") {
                let offset = segment["offset"].as_u64().expect("offset") as usize;
                let value = fixture_bytes(segment);
                data[offset..offset + value.len()].copy_from_slice(&value);
            }
            let parse = |data: &[u8]| -> Result<Value> {
                let payload = nvme_payload(data)?;
                if case["kind"] == "controller" {
                    Ok(serde_json::json!(parse_nvme_serial(payload)?))
                } else {
                    let ids = parse_nvme_namespace(payload)?;
                    Ok(serde_json::json!({"eui64": ids.eui64, "nguid": ids.nguid}))
                }
            };
            if case["error"] == true {
                assert!(parse(&data).is_err(), "{}", case["name"]);
            } else {
                assert_eq!(
                    parse(&data).expect("valid NVMe reply"),
                    case["expected"],
                    "{}",
                    case["name"]
                );
                for end in 0..data.len() {
                    assert!(
                        parse(&data[..end]).is_err(),
                        "{} prefix {end}",
                        case["name"]
                    );
                }
            }
        }
        let fixture: Value =
            serde_json::from_str(include_str!("../../tests/fixtures/wp-01/storage.json"))
                .expect("fixture JSON");
        assert_eq!(offset_of!(STORAGE_DEVICE_ID_DESCRIPTOR, Identifiers), 12);
        assert_eq!(offset_of!(STORAGE_IDENTIFIER, Identifier), 16);
        for case in fixture["descriptors"].as_array().expect("descriptor cases") {
            let data = fixture_bytes(case);
            let result = parse_descriptor(&data);
            if case["error"] == true {
                assert!(result.is_err(), "{}", case["name"]);
                continue;
            }
            let result = result.expect("valid descriptor");
            if case["expected"].is_null() {
                assert!(result.is_none());
            } else {
                let id = result.expect("identifier");
                assert_eq!(id.hex, case["expected"]["hex"], "{}", case["name"]);
                assert_eq!(id.decoded, case["expected"]["decoded"], "{}", case["name"]);
            }
            // Every prefix is truncated, even when its first identifier was valid.
            for end in 0..data.len() {
                assert!(
                    parse_descriptor(&data[..end]).is_err(),
                    "{} prefix {end}",
                    case["name"]
                );
            }
        }
    }

    #[test]
    fn drive_layout_and_extent_fixtures() {
        let fixture: Value =
            serde_json::from_str(include_str!("../../tests/fixtures/wp-01/storage.json"))
                .expect("fixture JSON");
        assert_eq!(offset_of!(DRIVE_LAYOUT_INFORMATION_EX, PartitionEntry), 48);
        assert_eq!(size_of::<PARTITION_INFORMATION_EX>(), 144);
        assert_eq!(
            offset_of!(PARTITION_INFORMATION_EX, Anonymous)
                + offset_of!(PARTITION_INFORMATION_GPT, PartitionId),
            48
        );
        for case in fixture["layouts"].as_array().expect("layout cases") {
            let data = fixture_bytes(case);
            let result = parse_layout(&data);
            if case["error"] == true {
                assert!(result.is_err(), "{}", case["name"]);
                continue;
            }
            let result = result.expect("valid layout");
            if case["expected"].is_null() {
                assert!(result.is_none());
            } else {
                let layout = result.expect("GPT/MBR");
                assert_eq!(layout.is_gpt, case["expected"]["is_gpt"]);
                assert_eq!(layout.disk_id, case["expected"]["disk_id"]);
                assert_eq!(
                    serde_json::to_value(layout.partition_guids).expect("GUID array"),
                    case["expected"]["partition_guids"]
                );
                for end in 0..data.len() {
                    assert!(
                        parse_layout(&data[..end]).is_err(),
                        "{} prefix {end}",
                        case["name"]
                    );
                }
            }
        }
        for case in fixture["extents"].as_array().expect("extent cases") {
            let result = single_extent_disk(&fixture_bytes(case));
            if case["error"] == true {
                assert!(result.is_err(), "{}", case["name"]);
            } else {
                assert_eq!(
                    result.expect("valid extents").map(u64::from),
                    case["expected"].as_u64(),
                    "{}",
                    case["name"]
                );
            }
        }
    }
}
