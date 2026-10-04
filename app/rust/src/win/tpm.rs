//! Read-only PCP endorsement-key inventory with crypt32 decoding and legacy fallbacks.

use super::{Error, Result, hash, process, record};
use std::time::Duration;
use windows::{
    Win32::Security::{Cryptography::*, OBJECT_SECURITY_INFORMATION},
    core::{PCSTR, PCWSTR},
};

struct Provider(NCRYPT_PROV_HANDLE);
impl Drop for Provider {
    fn drop(&mut self) {
        // SAFETY: This guard owns the provider returned by NCryptOpenStorageProvider.
        if let Err(e) = unsafe { NCryptFreeObject(self.0.into()) } {
            record(Error::from_win("NCryptFreeObject", e));
        }
    }
}

struct Store(HCERTSTORE);
impl Drop for Store {
    fn drop(&mut self) {
        // SAFETY: PCP transferred ownership of this certificate store to us.
        if let Err(e) = unsafe { CertCloseStore(Some(self.0), 0) } {
            record(Error::from_win("CertCloseStore", e));
        }
    }
}

struct Certificate(*const CERT_CONTEXT);
impl Drop for Certificate {
    fn drop(&mut self) {
        // SAFETY: The guard owns this context; null is accepted by crypt32.
        if !unsafe { CertFreeCertificateContext(Some(self.0)) }.as_bool() {
            record(Error::last("CertFreeCertificateContext"));
        }
    }
}

/// Reads the same default RSA EK and stores as Get-TpmEndorsementKeyInfo.
/// Ambiguous certificate sets and unsupported keys fall back as a whole.
pub fn native_ek() -> Result<Vec<(String, String)>> {
    let mut handle = NCRYPT_PROV_HANDLE::default();
    // SAFETY: The provider name is static and handle is writable. No key is created.
    unsafe { NCryptOpenStorageProvider(&mut handle, MS_PLATFORM_CRYPTO_PROVIDER, 0) }
        .map_err(|e| Error::from_win("NCryptOpenStorageProvider", e))?;
    let provider = Provider(handle);
    let key = property(&provider, NCRYPT_PCP_EKPUB_PROPERTY)?;
    let public_hash = rsa_public_hash(&key)?;
    // The cmdlet's ManufacturerCertificates comes from EKNVCERT; AdditionalCertificates
    // is EKCERT minus that collection. Do not substitute an RSA/ECC-specific store.
    let manufacturer = certificates(&provider, NCRYPT_PCP_EKNVCERT_PROPERTY)?;
    let additional = certificates(&provider, NCRYPT_PCP_EKCERT_PROPERTY)?;
    if manufacturer.len() != 1
        || additional.len() > 1
        || additional.iter().any(|cert| cert != &manufacturer[0])
    {
        return Err(Error::msg(
            "Native EK",
            "certificate set requires legacy formatting",
        ));
    }
    let mut fields = certificate_fields(&manufacturer[0])?;
    fields.insert(0, ("PublicKeyHash".into(), public_hash));
    Ok(fields)
}

fn property(provider: &Provider, name: PCWSTR) -> Result<Vec<u8>> {
    let mut size = 0;
    // SAFETY: Live provider and constant property name; this only queries a byte count.
    unsafe {
        NCryptGetProperty(
            provider.0.into(),
            name,
            None,
            &mut size,
            OBJECT_SECURITY_INFORMATION(0),
        )
    }
    .map_err(|e| Error::from_win("NCryptGetProperty", e))?;
    if size == 0 || size > 65536 {
        return Err(Error::msg("NCryptGetProperty", "invalid public-key size"));
    }
    let mut bytes = vec![0; size as usize];
    // SAFETY: The output slice and count are writable for their advertised sizes.
    unsafe {
        NCryptGetProperty(
            provider.0.into(),
            name,
            Some(&mut bytes),
            &mut size,
            OBJECT_SECURITY_INFORMATION(0),
        )
    }
    .map_err(|e| Error::from_win("NCryptGetProperty", e))?;
    if size == 0 || size as usize > bytes.len() {
        return Err(Error::msg(
            "NCryptGetProperty",
            "invalid returned public-key size",
        ));
    }
    bytes.truncate(size as usize);
    Ok(bytes)
}

pub(crate) fn rsa_public_hash(bytes: &[u8]) -> Result<String> {
    // CryptEncodeObjectEx receives no input length. Validate the CNG header first,
    // and copy into u32 storage so its BCRYPT_RSAKEY_BLOB is properly aligned.
    let invalid = || Error::msg("Native EK", "unsupported or truncated RSA public blob");
    if bytes.len() < 24 || bytes.len() > 65536 {
        return Err(invalid());
    }
    let mut header = [0u32; 6];
    for (value, chunk) in header.iter_mut().zip(bytes.as_chunks::<4>().0) {
        *value = u32::from_le_bytes(*chunk);
    }
    let [magic, bits, exponent, modulus, prime1, prime2] = header;
    if magic != BCRYPT_RSAPUBLIC_MAGIC.0
        || bits == 0
        || bits > 16384
        || modulus != bits.div_ceil(8)
        || exponent == 0
        || exponent > 8
        || prime1 != 0
        || prime2 != 0
        || 24 + exponent as usize + modulus as usize != bytes.len()
    {
        return Err(invalid());
    }
    let mut aligned = vec![0u32; bytes.len().div_ceil(4)];
    for (i, byte) in bytes.iter().enumerate() {
        aligned[i / 4] |= u32::from(*byte) << (8 * (i % 4));
    }
    let mut size = 0;
    // C# parity: Windows' EndorsementKey.GetPublicEndorsementKey encodes type 72
    // (CNG RSA public key) then hashes the resulting PKCS#1 DER, not the PCP blob.
    // SAFETY: The aligned blob's embedded lengths were checked against its allocation.
    unsafe {
        CryptEncodeObjectEx(
            X509_ASN_ENCODING,
            PCSTR(72usize as *const u8),
            aligned.as_ptr().cast(),
            CRYPT_ENCODE_OBJECT_FLAGS(0),
            None,
            None,
            &mut size,
        )
    }
    .map_err(|e| Error::from_win("CryptEncodeObjectEx", e))?;
    if size == 0 || size > 65536 {
        return Err(invalid());
    }
    let mut der = vec![0; size as usize];
    // SAFETY: Same validated input; der is writable for the size returned by crypt32.
    unsafe {
        CryptEncodeObjectEx(
            X509_ASN_ENCODING,
            PCSTR(72usize as *const u8),
            aligned.as_ptr().cast(),
            CRYPT_ENCODE_OBJECT_FLAGS(0),
            None,
            Some(der.as_mut_ptr().cast()),
            &mut size,
        )
    }
    .map_err(|e| Error::from_win("CryptEncodeObjectEx", e))?;
    if size == 0 || size as usize > der.len() {
        return Err(invalid());
    }
    hash::sha256_bytes(&der[..size as usize])
}

fn certificates(provider: &Provider, name: PCWSTR) -> Result<Vec<Vec<u8>>> {
    let mut handle = [0u8; size_of::<usize>()];
    let mut size = handle.len() as u32;
    // SAFETY: Store properties return an owned HCERTSTORE, not DER bytes. Query once
    // with a pointer-sized buffer, avoiding a size-query that could leak a store.
    unsafe {
        NCryptGetProperty(
            provider.0.into(),
            name,
            Some(&mut handle),
            &mut size,
            OBJECT_SECURITY_INFORMATION(0),
        )
    }
    .map_err(|e| Error::from_win("NCryptGetProperty (certificate store)", e))?;
    let store = Store(HCERTSTORE(usize::from_ne_bytes(handle) as *mut _));
    if size as usize != handle.len() || store.0.0.is_null() {
        return Err(Error::msg("Native EK", "invalid certificate store"));
    }
    let mut cert = Certificate(std::ptr::null());
    let mut result = Vec::new();
    loop {
        // SAFETY: The store is live; enumeration frees the previous context, which
        // is immediately replaced in the guard, including on end/error.
        cert.0 = unsafe { CertEnumCertificatesInStore(store.0, Some(cert.0)) };
        if cert.0.is_null() {
            let error = Error::last("CertEnumCertificatesInStore");
            if error.code != 0x80092004 {
                return Err(error);
            } // CRYPT_E_NOT_FOUND
            return Ok(result);
        }
        // SAFETY: crypt32 owns this live context and its encoded certificate buffer.
        let context = unsafe { &*cert.0 };
        if context.cbCertEncoded == 0 || context.cbCertEncoded > 65536 || result.len() >= 16 {
            return Err(Error::msg("Native EK", "certificate set exceeds bounds"));
        }
        // SAFETY: crypt32 provides cbCertEncoded valid bytes until the next enumeration.
        result.push(
            unsafe {
                std::slice::from_raw_parts(context.pbCertEncoded, context.cbCertEncoded as usize)
            }
            .to_vec(),
        );
    }
}

pub(crate) fn certificate_fields(der: &[u8]) -> Result<Vec<(String, String)>> {
    // SAFETY: crypt32 validates the supplied DER and owns the returned context.
    let cert = Certificate(unsafe { CertCreateCertificateContext(X509_ASN_ENCODING, der) });
    if cert.0.is_null() {
        return Err(Error::last("CertCreateCertificateContext"));
    }
    // SAFETY: A successful crypt32 context has a valid decoded CERT_INFO.
    let info = unsafe { &*(*cert.0).pCertInfo };
    if info.SerialNumber.cbData == 0 || info.SerialNumber.cbData > 20 {
        return Err(Error::msg("Native EK", "invalid certificate serial"));
    }
    // SAFETY: The serial buffer is owned by the live crypt32 context.
    let serial = unsafe {
        std::slice::from_raw_parts(info.SerialNumber.pbData, info.SerialNumber.cbData as usize)
    };
    let serial = serial
        .iter()
        .rev()
        .map(|b| format!("{b:02X}"))
        .collect::<String>();
    let mut thumbprint = [0u8; 20];
    let mut size = thumbprint.len() as u32;
    // SAFETY: The context is live; SHA-1's output buffer and byte count are writable.
    unsafe {
        CertGetCertificateContextProperty(
            cert.0,
            CERT_SHA1_HASH_PROP_ID,
            Some(thumbprint.as_mut_ptr().cast()),
            &mut size,
        )
    }
    .map_err(|e| Error::from_win("CertGetCertificateContextProperty", e))?;
    if size != 20 {
        return Err(Error::msg("Native EK", "invalid certificate thumbprint"));
    }
    // .NET X509Certificate.Issuer uses X500, reverse order and the default comma
    // separator/quoting. Keep the provider's literal CN=/O= extraction unchanged.
    let flags = CERT_STRING_TYPE(CERT_X500_NAME_STR.0 | CERT_NAME_STR_REVERSE_FLAG);
    // SAFETY: Issuer is a crypt32-validated name blob owned by the live context.
    let count = unsafe { CertNameToStrW(X509_ASN_ENCODING, &info.Issuer, flags, None) };
    if count <= 1 || count > 32768 {
        return Err(Error::msg("Native EK", "issuer unavailable"));
    }
    let mut issuer = vec![0u16; count as usize];
    // SAFETY: The name buffer is writable for the previously returned count.
    let written =
        unsafe { CertNameToStrW(X509_ASN_ENCODING, &info.Issuer, flags, Some(&mut issuer)) };
    if written != count {
        return Err(Error::msg("Native EK", "issuer size changed"));
    }
    let issuer = String::from_utf16(&issuer[..issuer.len() - 1])
        .map_err(|e| Error::msg("Native EK", e.to_string()))?;
    // Long/multi-line names can wrap in Format-List; the legacy parser keeps only
    // the first line. Use that path rather than inventing its console width here.
    if issuer.contains(['\r', '\n']) || issuer.encode_utf16().count() > 48 {
        return Err(Error::msg("Native EK", "issuer requires legacy formatting"));
    }
    Ok(vec![
        ("Serial Number".into(), serial),
        (
            "Thumbprint".into(),
            thumbprint.iter().map(|b| format!("{b:02X}")).collect(),
        ),
        ("Issuer".into(), issuer),
    ])
}

/// A PowerShell result retaining nonterminating errors alongside usable partial output.
pub struct PowerShellOutput {
    /// The legacy formatted output, without whitespace or identifier normalization.
    pub text: String,
    /// A child-process error, also retained when stdout contains usable data.
    pub failure: Option<Error>,
}

/// Reads the legacy Get-Tpm status with a 15-second process deadline.
pub fn powershell_status() -> Result<PowerShellOutput> {
    run("Get-Tpm", "Get-Tpm")
}

/// Reads the legacy endorsement-key formatting with a 15-second process deadline.
pub fn powershell_ek() -> Result<PowerShellOutput> {
    // C# parity: Hardware/TpmInfo.cs:174. Keep formatted text for every native
    // failure, incomplete field set, and ambiguous certificate ordering.
    run(
        "Get-TpmEndorsementKeyInfo",
        "Get-TpmEndorsementKeyInfo -Hash 'Sha256' | Format-List",
    )
}

fn run(op: &'static str, command: &str) -> Result<PowerShellOutput> {
    let directory = process::system32("powershell.exe");
    if !directory.is_absolute() {
        return Err(Error::msg(
            op,
            "the System32 directory could not be resolved",
        ));
    }
    let parent = directory
        .parent()
        .ok_or_else(|| Error::msg(op, "the System32 directory has no parent"))?;
    let exe = parent.join(r"WindowsPowerShell\v1.0\powershell.exe");
    // C# parity: Hardware/TpmInfo.cs:282 runs `-Command` with the user profile.
    // `-NoProfile` is a deliberate improvement: a broken or slow profile must not
    // add stderr noise or delay; cmdlet and Format-List output stay the same.
    // The shared runner owns the process/job/pipes and kills the tree on timeout.
    let output = process::run(
        &exe,
        &["-NoProfile", "-NonInteractive", "-Command", command],
        Duration::from_secs(15),
        &process::Cancel::new(),
    )
    .map_err(|error| Error::msg(op, error))?;
    let failure = if output.code != 0 || !output.stderr.trim().is_empty() {
        Some(Error {
            op,
            code: output.code as u32,
            detail: output.stderr.trim().to_owned(),
        })
    } else {
        None
    };
    // C# parity: Hardware/TpmInfo.cs:175-178,218-222. C# never reads stderr, so a
    // cmdlet error with empty stdout keeps the provider's fixed text; the error
    // goes to diagnostics. Only start failures and timeouts above are errors.
    Ok(PowerShellOutput {
        text: output.stdout,
        failure,
    })
}
