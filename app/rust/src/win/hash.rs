//! Owned by WP-13: BCrypt SHA-256 hashing.

use super::{Error, Result, record, wide};
use std::{
    fs::File,
    io::{self, Read, Seek, SeekFrom},
    path::Path,
};
use windows::{
    Win32::{
        Foundation::{NTSTATUS, RtlNtStatusToDosError},
        Security::Cryptography::{
            BCRYPT_ALG_HANDLE, BCRYPT_HASH_HANDLE, BCRYPT_OPEN_ALGORITHM_PROVIDER_FLAGS,
            BCRYPT_SHA256_ALGORITHM, BCRYPT_USE_SYSTEM_PREFERRED_RNG, BCryptCloseAlgorithmProvider,
            BCryptCreateHash, BCryptDestroyHash, BCryptFinishHash, BCryptGenRandom, BCryptHashData,
            BCryptOpenAlgorithmProvider,
        },
        System::Diagnostics::Debug::{
            FORMAT_MESSAGE_FROM_SYSTEM, FORMAT_MESSAGE_IGNORE_INSERTS, FormatMessageW,
        },
    },
    core::{PCWSTR, PWSTR},
};

struct Algorithm(BCRYPT_ALG_HANDLE);

impl Drop for Algorithm {
    fn drop(&mut self) {
        // SAFETY: This guard uniquely owns a successfully opened algorithm provider.
        if let Err(error) = status("BCryptCloseAlgorithmProvider", unsafe {
            BCryptCloseAlgorithmProvider(self.0, 0)
        }) {
            record(error);
        }
    }
}

struct Hash {
    handle: BCRYPT_HASH_HANDLE,
    _algorithm: Algorithm,
}

impl Hash {
    fn new() -> Result<Self> {
        let mut handle = BCRYPT_ALG_HANDLE::default();
        // SAFETY: handle is writable; the algorithm name is a static NUL-terminated string.
        status("BCryptOpenAlgorithmProvider", unsafe {
            BCryptOpenAlgorithmProvider(
                &mut handle,
                BCRYPT_SHA256_ALGORITHM,
                PCWSTR::null(),
                BCRYPT_OPEN_ALGORITHM_PROVIDER_FLAGS(0),
            )
        })?;
        let algorithm = Algorithm(handle);
        let mut handle = BCRYPT_HASH_HANDLE::default();
        // SAFETY: The live provider owns the algorithm; CNG allocates the hash object
        // when its buffer is None (supported since Windows 7). The guard destroys it.
        status("BCryptCreateHash", unsafe {
            BCryptCreateHash(algorithm.0, &mut handle, None, None, 0)
        })?;
        Ok(Self {
            handle,
            _algorithm: algorithm,
        })
    }

    fn add(&self, bytes: &[u8]) -> Result<()> {
        // SAFETY: The hash remains live and the input slice advertises its actual length.
        status("BCryptHashData", unsafe {
            BCryptHashData(self.handle, bytes, 0)
        })
    }

    fn finish(self) -> Result<String> {
        let mut digest = [0_u8; 32];
        // SAFETY: SHA-256 always produces 32 bytes; digest is writable for that length.
        status("BCryptFinishHash", unsafe {
            BCryptFinishHash(self.handle, &mut digest, 0)
        })?;
        // C# parity: Services/AutoUpdateService.cs:102,119 (lower-case hexadecimal).
        Ok(hex(&digest))
    }
}

impl Drop for Hash {
    fn drop(&mut self) {
        // SAFETY: The guard uniquely owns the hash and its algorithm is still alive.
        if let Err(error) = status("BCryptDestroyHash", unsafe {
            BCryptDestroyHash(self.handle)
        }) {
            record(error);
        }
    }
}

fn status(op: &'static str, result: NTSTATUS) -> Result<()> {
    if result.0 >= 0 {
        Ok(())
    } else {
        // SAFETY: The translator accepts any NTSTATUS and returns its Win32 message code.
        let code = unsafe { RtlNtStatusToDosError(result) };
        let mut buffer = [0_u16; 2048];
        // SAFETY: buffer is writable for the advertised UTF-16 length; no inserts are used.
        let count = unsafe {
            FormatMessageW(
                FORMAT_MESSAGE_FROM_SYSTEM | FORMAT_MESSAGE_IGNORE_INSERTS,
                None,
                code,
                0,
                PWSTR(buffer.as_mut_ptr()),
                buffer.len() as u32,
                None,
            )
        };
        Err(Error {
            op,
            code: result.0 as u32,
            detail: if count == 0 {
                "No system error description available.".to_owned()
            } else {
                wide::from_wide(&buffer[..count as usize])
                    .trim_end()
                    .to_owned()
            },
        })
    }
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

/// Preserves a file operation's native error code and system description.
pub(crate) fn io_error(op: &'static str, error: io::Error) -> Error {
    Error {
        op,
        code: error.raw_os_error().unwrap_or(0) as u32,
        detail: error.to_string(),
    }
}

/// Computes the lower-case SHA-256 of a file from its first byte, using CNG.
pub fn sha256_file(file: &File) -> Result<String> {
    let mut reader = file;
    reader
        .seek(SeekFrom::Start(0))
        .map_err(|e| io_error("Seek update file", e))?;
    let hash = Hash::new()?;
    let mut buffer = [0_u8; 8192];
    loop {
        let count = reader
            .read(&mut buffer)
            .map_err(|e| io_error("Read update file", e))?;
        if count == 0 {
            break;
        }
        hash.add(&buffer[..count])?;
    }
    hash.finish()
}

/// Opens and hashes an existing executable without loading or executing it.
pub fn sha256(path: &Path) -> Result<String> {
    let file = File::open(path).map_err(|e| io_error("Open executable for SHA256", e))?;
    sha256_file(&file)
}

/// Computes SHA-256 for bytes already in memory, without a second implementation.
pub(crate) fn sha256_bytes(bytes: &[u8]) -> Result<String> {
    let hash = Hash::new()?;
    hash.add(bytes)?;
    hash.finish()
}

/// Generates an unpredictable name component with the Windows system RNG.
pub(crate) fn random_name() -> Result<String> {
    let mut bytes = [0_u8; 16];
    // SAFETY: The system provider needs no handle; bytes is a writable 16-byte slice.
    status("BCryptGenRandom", unsafe {
        BCryptGenRandom(None, &mut bytes, BCRYPT_USE_SYSTEM_PREFERRED_RNG)
    })?;
    Ok(hex(&bytes))
}

fn invalid_pe() -> Error {
    Error::msg("Validate update PE", "not a valid x64 PE executable")
}

fn read_at(file: &File, offset: u64, size: usize) -> Result<Vec<u8>> {
    let mut reader = file;
    reader
        .seek(SeekFrom::Start(offset))
        .map_err(|e| io_error("Seek PE header", e))?;
    let mut bytes = vec![0; size];
    reader
        .read_exact(&mut bytes)
        .map_err(|e| io_error("Read PE header", e))?;
    Ok(bytes)
}

/// Validates the complete AMD64 PE32+ header and all file/image ranges before install.
pub fn validate_x64_pe(file: &File) -> Result<()> {
    let size = file
        .metadata()
        .map_err(|e| io_error("Read PE size", e))?
        .len();
    if size < 64 {
        return Err(invalid_pe());
    }
    let dos = read_at(file, 0, 64)?;
    if &dos[..2] != b"MZ" {
        return Err(invalid_pe());
    }
    let offset = number(&dos, 60, 4)?;
    if offset < 64 || offset + 24 > size {
        return Err(invalid_pe());
    }
    let mut headers = read_at(file, offset, 24)?;
    let sections = number(&headers, 6, 2)?;
    if !(1..=96).contains(&sections) {
        return Err(invalid_pe());
    }
    let rest = number(&headers, 20, 2)? + sections * 40;
    if offset + 24 + rest > size {
        return Err(invalid_pe());
    }
    headers.extend(read_at(file, offset + 24, rest as usize)?);
    validate_pe_headers(offset, &headers, size)
}

fn number(bytes: &[u8], offset: usize, width: usize) -> Result<u64> {
    let field = bytes.get(offset..offset + width).ok_or_else(invalid_pe)?;
    Ok(field
        .iter()
        .enumerate()
        .fold(0, |value, (i, &byte)| value | ((byte as u64) << (i * 8))))
}

fn validate_pe_headers(offset: u64, bytes: &[u8], file_size: u64) -> Result<()> {
    if offset < 64 || bytes.get(..4) != Some(b"PE\0\0") || number(bytes, 4, 2)? != 0x8664 {
        return Err(invalid_pe());
    }
    let sections = number(bytes, 6, 2)? as usize;
    let optional_size = number(bytes, 20, 2)? as usize;
    let characteristics = number(bytes, 22, 2)?;
    if !(1..=96).contains(&sections)
        || characteristics & 2 == 0
        || characteristics & 0x100 != 0
        || characteristics & 0x2000 != 0
        || optional_size < 112
        || bytes.len() < 24 + optional_size + sections * 40
    {
        return Err(invalid_pe());
    }
    let optional = &bytes[24..24 + optional_size];
    let entry = number(optional, 16, 4)?;
    let image_base = number(optional, 24, 8)?;
    let section_align = number(optional, 32, 4)?;
    let file_align = number(optional, 36, 4)?;
    let image_size = number(optional, 56, 4)?;
    let header_size = number(optional, 60, 4)?;
    let directories = number(optional, 108, 4)? as usize;
    if number(optional, 0, 2)? != 0x20b
        || !matches!(number(optional, 68, 2)?, 2 | 3)
        || !section_align.is_power_of_two()
        || !file_align.is_power_of_two()
        || file_align > 65536
        || section_align < file_align
        || (file_align < 512 && file_align != section_align)
        || image_base == 0
        || !image_base.is_multiple_of(65536)
        || entry == 0
        || entry >= image_size
        || image_size == 0
        || !image_size.is_multiple_of(section_align)
        || header_size < offset + bytes.len() as u64
        || header_size > file_size
        || !header_size.is_multiple_of(file_align)
        || directories > 16
        || 112 + directories * 8 > optional_size
    {
        return Err(invalid_pe());
    }
    let mut ranges = Vec::with_capacity(sections);
    let mut raw_ranges = Vec::with_capacity(sections);
    let mut entry_executable = false;
    for section in bytes[24 + optional_size..24 + optional_size + sections * 40]
        .as_chunks::<40>()
        .0
    {
        let virtual_size = number(section, 8, 4)?;
        let rva = number(section, 12, 4)?;
        let raw_size = number(section, 16, 4)?;
        let raw = number(section, 20, 4)?;
        let end = rva + virtual_size.max(raw_size);
        if rva < header_size
            || !rva.is_multiple_of(section_align)
            || end > image_size
            || (raw_size > 0
                && (raw < header_size
                    || !raw.is_multiple_of(file_align)
                    || !raw_size.is_multiple_of(file_align)
                    || raw + raw_size > file_size))
        {
            return Err(invalid_pe());
        }
        if ranges.iter().any(|&(a, b)| rva < b && end > a)
            || (raw_size > 0
                && raw_ranges
                    .iter()
                    .any(|&(a, b)| raw < b && raw + raw_size > a))
        {
            return Err(invalid_pe());
        }
        entry_executable |=
            (rva..end).contains(&entry) && number(section, 36, 4)? & 0x20000000 != 0;
        ranges.push((rva, end));
        if raw_size > 0 {
            raw_ranges.push((raw, raw + raw_size));
        }
    }
    if !entry_executable {
        return Err(invalid_pe());
    }
    for index in 0..directories {
        let address = number(optional, 112 + index * 8, 4)?;
        let size = number(optional, 116 + index * 8, 4)?;
        if address == 0 && size == 0 {
            continue;
        }
        if address == 0 || size == 0 {
            return Err(invalid_pe());
        }
        if index == 4 {
            // The certificate directory uses a file offset, not an RVA.
            if !address.is_multiple_of(8) || address < header_size || address + size > file_size {
                return Err(invalid_pe());
            }
        } else if !(address < header_size && address + size <= header_size)
            && !ranges
                .iter()
                .any(|&(a, b)| address >= a && address + size <= b)
        {
            return Err(invalid_pe());
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> Vec<u8> {
        let hex: String = include_str!("../../tests/fixtures/wp-13/x64-pe.hex")
            .chars()
            .filter(|ch| !ch.is_whitespace())
            .collect();
        hex.as_bytes()
            .as_chunks::<2>()
            .0
            .iter()
            .map(|pair| {
                u8::from_str_radix(std::str::from_utf8(pair).expect("fixture ASCII"), 16)
                    .expect("fixture hex")
            })
            .collect()
    }

    fn headers() -> Vec<u8> {
        fixture()[128..432].to_vec()
    }

    fn put(bytes: &mut [u8], offset: usize, value: u32, width: usize) {
        bytes[offset..offset + width].copy_from_slice(&value.to_le_bytes()[..width]);
    }

    #[test]
    fn x64_pe_headers_reject_truncation_architecture_and_invalid_sections() {
        let original = headers();
        validate_pe_headers(128, &original, 1024).expect("fabricated x64 PE");
        for length in 0..original.len() {
            assert!(
                validate_pe_headers(128, &original[..length], 1024).is_err(),
                "truncated length {length}"
            );
        }
        for (offset, value, width) in [
            (0, 0, 4),
            (4, 0x14c, 2),
            (6, 0, 2),
            (6, 97, 2),
            (20, 111, 2),
            (20, 65535, 2),
            (22, 0, 2),
            (22, 0x102, 2),
            (22, 0x2002, 2),
            (24, 0x10b, 2),
            (24 + 16, 0, 4),
            (24 + 16, 0x3000, 4),
            (24 + 32, 3, 4),
            (24 + 36, 0, 4),
            (24 + 56, 4096, 4),
            (24 + 60, 256, 4),
            (24 + 68, 1, 2),
            (24 + 68, 10, 2),
            (24 + 108, 17, 4),
            (264 + 12, 0xfffff000, 4),
            (264 + 16, 1024, 4),
            (264 + 20, 0, 4),
            (264 + 36, 0x40000020, 4),
        ] {
            let mut bytes = original.clone();
            put(&mut bytes, offset, value, width);
            assert!(
                validate_pe_headers(128, &bytes, 1024).is_err(),
                "invalid field at {offset}: {value}"
            );
        }
    }

    #[test]
    fn pe_directories_use_mapped_rvas_but_certificates_use_file_offsets() {
        let mut bytes = headers();
        put(&mut bytes, 24 + 112, 4096, 4);
        put(&mut bytes, 24 + 116, 1, 4);
        validate_pe_headers(128, &bytes, 1024).expect("mapped directory");
        put(&mut bytes, 24 + 112, 8191, 4);
        assert!(validate_pe_headers(128, &bytes, 1024).is_err());
        let mut bytes = headers();
        put(&mut bytes, 24 + 112 + 4 * 8, 1024, 4);
        put(&mut bytes, 24 + 116 + 4 * 8, 8, 4);
        validate_pe_headers(128, &bytes, 1032).expect("certificate file offset");
        assert!(validate_pe_headers(128, &bytes, 1024).is_err());
        put(&mut bytes, 24 + 112 + 4 * 8, 1025, 4);
        assert!(validate_pe_headers(128, &bytes, 1040).is_err());
    }

    #[test]
    fn overlapping_pe_sections_are_rejected() {
        let mut bytes = headers();
        bytes.extend_from_within(264..304);
        put(&mut bytes, 6, 2, 2);
        assert!(validate_pe_headers(128, &bytes, 1536).is_err());
    }

    #[test]
    fn cng_sha256_known_vectors() {
        let error = status("BCryptHashData", NTSTATUS(0xC000_000D_u32 as i32))
            .expect_err("STATUS_INVALID_PARAMETER is a failure");
        assert_eq!(error.code, 0xC000_000D);
        // Query the OS's localized Win32 counterpart, independently of the NTSTATUS formatter.
        let expected =
            windows::core::Error::from_hresult(windows::core::HRESULT::from_win32(87)).message();
        assert_eq!(error.detail, expected.trim_end());
        assert_eq!(
            sha256_bytes(b"").expect("CNG"),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        assert_eq!(
            sha256_bytes(b"abc").expect("CNG"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }
}
