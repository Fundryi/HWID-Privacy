# fTPM Identity Reset Guide (AMD AM5 + Intel status)

> **Warning**: This guide changes TPM state. Read all of it before you touch anything.
> **Warning**: If you use BitLocker or any drive encryption, suspend it or save your recovery key first. A TPM reset can lock you out of your drives.
> **Warning**: BIOS flashing has real risk. Use only the exact BIOS file for your exact board revision. A power cut during a flash can kill the board.
>
> Evidence grades used here: **[C]** confirmed first hand by a named user with details, **[A]** agent-verified live during research (2026-08-21), **[CC]** community consensus, many reports, **[S]** single unverified claim. Untested steps are marked.

> [!NOTE]
> **[A]** grades refer to linked primary sources or this repository's source.

---

## Table of Contents

- [Before you reset](#before-you-reset)
- [How the AMD fTPM identity actually works](#how-the-amd-ftpm-identity-actually-works)
- [Measure properly or you will fool yourself](#measure-properly-or-you-will-fool-yourself)
- [Windows views and HWIDChecker baseline](#windows-views-and-hwidchecker-baseline)
- [Method A: TPM-B firmware flash cycle (Gigabyte, works on other brands too)](#method-a-tpm-b-firmware-flash-cycle)
  - [Step 0: Prepare (do not skip)](#step-0-prepare-do-not-skip)
  - [Step 1: Pick the right BIOS pair](#step-1-pick-the-right-bios-pair)
  - [Step 2: Flash](#step-2-flash)
  - [Known results per board (from public reports)](#known-results-per-board-from-public-reports)
  - [Failure modes and how to avoid them](#failure-modes-and-how-to-avoid-them)
- [Method B: Pluton toggle (unverified, do not trust yet)](#method-b-pluton-toggle-unverified)
- [Method C: dTPM module (different identity, gets flagged)](#method-c-dtpm-module)
- [What does NOT change the EK](#what-does-not-change-the-ek)
- [Check your certificate after any rotation](#check-your-certificate-after-any-rotation)
- [Certificate retrieval and trust](#certificate-retrieval-and-trust)
- [Intel: Z790 vs Z890](#intel-z790-vs-z790-era-method-vs-z890)
- [Evidence ledger](#evidence-ledger)
- [Sources](#sources)

---

## Before you reset

Read the [TPM identity terms](../tpm-spoofing/tpm-spoofing.md#tpm-identity-and-terminology), [clear precautions](../tpm-spoofing/tpm-spoofing.md#what-clearing-the-tpm-changes), and [firmware-update limits](../tpm-spoofing/tpm-spoofing.md#firmware-updates-and-ek-continuity) first. Complete those precautions and capture the baseline below before any clear, firmware flash, or switch between fTPM, dTPM, and Pluton.

## How the AMD fTPM identity actually works

Three facts drive everything below.

1. The AMD fTPM endorsement key (EK) is not random per reset. All evidence says it is derived from secrets inside your CPU plus the running fTPM firmware version. Same CPU plus same firmware version gives back the same identity.
   - A user on a Gigabyte X570 Aorus Elite downgraded and got his original hashes back exactly, then upgraded and got the same second set again. Two fixed identities, not infinite ones. **[C]**
   - Multiple users flashed BIOS versions that did not carry an fTPM firmware change. Their identity stayed the same. **[CC]**
2. Wiping TPM state alone does not make a new EK. The AMI prompt ("Press Y to reset fTPM") rebuilds NV structures. It does not reseed the endorsement key by itself. **[A]** from primary sources, confirmed by reports where Y was pressed with no firmware change and hashes stayed identical.
3. Windows fetches the EK certificate online from `https://ftpm.amd.com/pki/aia/<hash-of-EKpub>` at provisioning time and stores it in TPM NV. That server serves pre-registered certificates only. During live testing it returned HTTP 404 for keys it does not know and served stable certs for real chips. It does not mint certificates for arbitrary new keys on demand. **[A]**

Consequence: a rotation path is only useful if your new EK also gets a valid certificate. If the server has never seen your new key hash, you get no manufacturer cert. Attestation-based checks then fail instead of pass. Check this after every attempt (section below).

> [!IMPORTANT]
> Primary sources establish that a normal clear does not change the EPS and that AMD firmware TPMs need access to AMD's certificate-retrieval endpoint. They do not document AMD's EK derivation inputs, the endpoint's per-key suffix, or whether certificates are pre-registered rather than created on request. Treat those AMD-specific explanations above as community or live-service observations, not **[A]** facts. The TCG field-upgrade requirement also says the original manufacturer-certified EK must remain reproducible for the same template until `TPM2_ChangeEPS`. **[A]** [TCG architecture](https://trustedcomputinggroup.org/wp-content/uploads/Trusted-Platform-Module-2.0-Library-Part-1-Architecture_Version-185_pub.pdf) and [Windows Autopilot requirements](https://learn.microsoft.com/en-us/autopilot/requirements)

---

## Measure properly or you will fool yourself

`Get-TpmEndorsementKeyInfo` reads a Windows cache in many cases. People have posted "changed" hashes that were stale cache reads.

Before and after every operation, record:

```powershell
# Run as admin. Record ALL three lines each time.
Get-TpmEndorsementKeyInfo -HashAlgorithm Sha256 | Format-List PublicKeyHash, ManufacturerCertificates, AdditionalCertificates

# Export the manufacturer cert and dump its serial and dates. Serial + NotBefore is your true fingerprint.
$ek = Get-TpmEndorsementKeyInfo -HashAlgorithm Sha256
$ek.ManufacturerCertificates | Export-Certificate -FilePath "$env:TEMP\ek.cer"
certutil -dump "$env:TEMP\ek.cer" | Select-String "Serial|NotBefore|NotAfter|Issuer"
```

If you can boot Linux, this bypasses Windows cache completely:

```bash
tpm2_getcap handles-persistent
tpm2_readpublic -c 0x81010001 -f pem -o ek.pem        # then sha256 over the DER
tpm2_nvread 0x01c00002 -C o -o ekcert.der             # RSA EK cert if populated
openssl x509 -inform der -in ekcert.der -text -noout  # serial, NotBefore, issuer
```

Also run an MMIO-based checker if available. Community reports say cached readings and MMIO readings can disagree. **[CC]**

---

## Windows views and HWIDChecker baseline

Use the commands and interpretation in [Inspect the TPM in Windows](../tpm-spoofing/tpm-spoofing.md#inspect-the-tpm-in-windows) and [Verify with HWIDChecker.exe](../tpm-spoofing/tpm-spoofing.md#verify-with-hwidcheckerexe) before and after each attempt. Preserve the raw PowerShell certificate collections. HWIDChecker is a convenient paired view, not an independent measurement or a complete certificate inventory.

---

## Method A: TPM-B firmware flash cycle

This is the only AMD rotation path with multiple independent first-hand confirmations. It works because some BIOS updates ship a new version of the fTPM firmware itself ("TPM-B FW"). When the board boots with a different fTPM firmware version, the AMI screen appears and offers to reset fTPM. Accepting it brings up the TPM under the new firmware version, which derives a different EK.

**Status**: identity change confirmed on the boards listed below. Certificate validity after rotation is UNTESTED in public. Verify yours with the check in the next section. If your new key has no server cert, this method gave you an identity that fails attestation, and you should cycle back.

### Step 0: Prepare (do not skip)

1. Suspend BitLocker, or save recovery keys for every encrypted drive. Conditions: if you skip this and press Y later, encrypted drives will demand the recovery key.
2. Plug in a USB stick formatted FAT32.
3. Download BIOS files ONLY from your board's official support page. Match your exact model AND board revision (rev 1.x vs rev 2.x are different files).
4. Note your current BIOS version (BIOS main screen, or `Get-ComputerInfo | Select BiosSMBIOSBIOSVersion`).
5. Unzip the BIOS file onto the USB root. The file name must match your board.

### Step 1: Pick the right BIOS pair

Open your board's support page BIOS list. Look for a changelog entry like this (real example, Gigabyte B650M DS3H F67i, 2025-09-03):

> Update TPM-B FW for Raven2/ Picasso, Cezanne, Vermeer/ Matisse & Renoir CPUs
> Fix TPM2.0's out-of-bounds read vulnerability (CVE-2025-2884)

Any version jump that includes a TPM-B FW update is a candidate. You need TWO versions: your current one and one with the TPM-B update. The rotation comes from moving between them. Moving again moves you back.

AMD shipped the CVE-2025-2884 fTPM fixes to OEMs between 2024-09 and 2025-04 depending on platform (AMD-SB-4011), so most vendor BIOSes from mid 2025 onward carry a TPM-B change relative to older releases. **[A]**

### Step 2: Flash

1. Reboot into BIOS (Del on most boards). Open Q-Flash (F8 on Gigabyte, or from the BIOS menu).
2. Select the BIOS file from the USB stick. Confirm. Do not touch power during the flash.
3. On reboot, watch for this screen:

   > American Megatrends ... New CPU installed, fTPM/PSP NV corrupted or fTPM/PSP NV structure changed.
   > Press Y to reset fTPM ... Press N to keep previous fTPM record ...

4. Press Y. (This is the point of no return for BitLocker keys. Step 0 must be done.)
5. Boot into Windows. Run the paired measurement from the section above. Compare EK hash, cert serial, NotBefore.

To get a third state later, flash back toward your old version and accept the prompt again. Expect to land back on your original identity, because derivation is deterministic. **[C]** on X570, expect similar elsewhere.

### Known results per board (from public reports)

Board | Result | Grade
---|---|---
Gigabyte X870E Aorus Elite WiFi7 | Works | [C] relayed by thread OP
Gigabyte B650 Gaming X AX V2 | Works | [C] relayed
Gigabyte B650 Aorus AX B2 | Reported working | [CC]
Gigabyte B550 Gaming X V2 rev 1.3 | Works (serials changed) | [C]
Gigabyte X570 Aorus Elite + 5900X | Changes, but deterministic two-set cycle | [C]
MSI B550 Gaming Gen 3 | Works (latest BIOS) | [C]
ASUS AM4 (7th gen era board) | Works | [C]
MSI B450 Tomahawk Max | Changed once, then stopped responding to the trick | [C]
ASRock B550M Steel Legend | First report said changed, follow-up said unchanged after upgrade/downgrade | conflicting [C]/[C]
Gigabyte B650M Gaming Plus WiFi | No prompt, unchanged | [C]
Gigabyte B650M DS3H | F30 to F35 no change (that pair carries no TPM-B change); F67i is the right target | [C]
Gigabyte B650M D3HP | Lowest-to-highest without TPM-B pair, unchanged | [C]
Gigabyte B450M Gaming rev 1.x | Unchanged, no prompt | [C]
Gigabyte B650 Aorus Elite AX rev 1.2 | No luck | [C] relayed
ASRock B850M Steel Legend WiFi | No such option/update found | [C]

Pattern: the method works when your old-to-new pair actually contains an fTPM firmware version bump. It fails otherwise. "Pure luck" in community words equals "check the changelog". **[CC]**

### Failure modes and how to avoid them

- Wrong file for your board revision: brick risk. Match model and rev exactly.
- Downgrading too far on AM4: some newer CPUs cannot boot old AGESA versions. One 5700X3D owner could not post on old BIOS. If you have a late CPU, prefer the smallest step that still contains the TPM-B change.
- Some boards need an intermediate version when jumping across big ranges (one B450 owner had to pass through F32). If Q-Flash refuses a file, step through intermediate versions.
- Pressing N keeps the old state. Nothing rotates. You must press Y.
- Laptop OEMs may block rollback entirely (an HP case could not roll back at all).

---

## Method B: Pluton toggle (unverified)

On AM5 Gigabyte boards: Advanced -> Miscellaneous -> Trusted Platform Module. Options: Auto, Disabled, Enable dTPM, Enable ASP fTPM, Enable Pluton fTPM. ASUS, MSI and Supermicro expose the same choice under their own names. **[A]**

The claim from April 2026: switching to Pluton and back generates a completely fresh EK every time, while the ASP fTPM identity stays permanent. **[S]**

> [!WARNING]
> This **[S]** procedure is untested. The available screenshots do not contain a valid before-and-after pair, and no matching certificate was verified. Do not rely on it as an identity-rotation method.

Why we do not trust it yet:

- The poster's own attached screenshot album shows the SAME hashes twice, not a before-and-after pair.
- His measurement was the cache-prone PowerShell reading.
- Nobody in the thread verified a certificate came along with the new key.
- Pluton does have its own live PKI (AMD "Pluton Global Factory ICA" verified reachable in 2026-08, plus a Microsoft Pluton Root CA 2021). But whether each re-activation mints a fresh chain-valid cert is publicly undocumented. **[A]**
- Microsoft states that starting with 2026 silicon, Pluton no longer acts as the TPM on AMD platforms. Building on this path has a short shelf life. **[A]**

If you try it anyway: full paired measurement, cert serial check, and the server-cert lookup from the next section. Report results with both screenshots.

---

## Method C: dTPM module

A discrete TPM module in the header carries its own factory EK and certificate. Mechanically this is a different chip's identity. Two problems:

- Strict telemetry stacks flag dTPM presence, and since 2025-04-04 at least one validator enforces fTPM when it sees the flag. **[CC]**
- Modules are interchangeable commodities. Their certs come from Infineon, Nuvoton, ST and friends, which makes provenance look odd for a desktop build that should have firmware TPM.

Not recommended. Documented here so you do not waste money on it.

---

## What does NOT change the EK

Save yourself hours. These do nothing to the endorsement identity:

- Windows "Clear TPM", `Clear-Tpm`, or pressing Y at the AMI prompt when the fTPM firmware version did not change. **[A]** spec-level, plus field reports.
- CMOS clear, battery pull, BIOS settings reset. **[CC]**
- Reinstalling Windows. (One Intel user reported rotated keys after a fresh install plus BIOS flash, but that report is confounded with the flash itself.) **[S]**
- BIOS updates whose changelog has no TPM-B/fTPM entry. **[CC]**
- On Z890-class Intel: everything listed in the Intel section below.

> [!WARNING]
> Reinstalling Windows as an EK-rotation procedure is **[S]** and untested. The cited report also included a BIOS flash, so it does not isolate the reinstall as the cause.

---

## Check your certificate after any rotation

Run this after ANY identity change. It asks AMD's live server whether your current EK has a registered certificate.

```powershell
# PowerShell as admin. Best effort diagnostic for RSA EKs.
$ek = Get-TpmEndorsementKeyInfo -HashAlgorithm Sha256
$p  = $ek.PublicKey.ExportParameters($false)

$pre = [byte[]](0x00,0x00,0x22,0x22)
$exp = [byte[]](0x00,0x01,0x00,0x01)   # e = 65537
$sha = [System.Security.Cryptography.SHA256]::Create()
$h = ($sha.ComputeHash(($pre + $exp + $p.Modulus)))[0..15]
$url = "https://ftpm.amd.com/pki/aia/" + (($h | ForEach-Object { $_.ToString('X2') }) -join '')
$url
try {
    $r = Invoke-WebRequest -Uri $url -UseBasicParsing -TimeoutSec 20
    "HTTP $($r.StatusCode): certificate EXISTS server-side ($($r.RawContentLength) bytes)"
} catch {
    $code = $null; if ($_.Exception.Response) { $code = [int]$_.Exception.Response.StatusCode }
    "HTTP $code: NO certificate registered for this EK"
}
```

> [!WARNING]
> **Diagnostic evidence limit:** the API contract is **[A]**, based on Microsoft's documented `AsnEncodedData` output contract and .NET RSA import APIs. The procedure as a whole remains **[S]** because the real cmdlet output could not be parsed without elevation and AMD does not publish the endpoint-suffix construction. Use PowerShell 7 or later as administrator. Do not treat a request failure as proof that a certificate is absent.

```powershell
# PowerShell 7+ as administrator. Diagnostic for RSA EKs only.
$ek = Get-TpmEndorsementKeyInfo -HashAlgorithm Sha256
if ($ek -is [string]) { throw $ek }
if (-not $ek.IsPresent -or $null -eq $ek.PublicKey) {
    throw "Windows did not return an endorsement public key."
}

$rsa = [System.Security.Cryptography.RSA]::Create()
try {
    $bytesRead = 0
    try {
        $rsa.ImportSubjectPublicKeyInfo($ek.PublicKey.RawData, [ref]$bytesRead)
    } catch [System.Security.Cryptography.CryptographicException] {
        $rsa.Dispose()
        $rsa = [System.Security.Cryptography.RSA]::Create()
        $bytesRead = 0
        $rsa.ImportRSAPublicKey($ek.PublicKey.RawData, [ref]$bytesRead)
    }
    if ($bytesRead -ne $ek.PublicKey.RawData.Length) {
        throw "The RSA parser did not consume the complete public key."
    }
    $p = $rsa.ExportParameters($false)
} finally {
    $rsa.Dispose()
}

if ($p.Exponent.Length -gt 4) { throw "Unsupported RSA exponent size." }
$exp = [byte[]]::new(4)
[Array]::Copy($p.Exponent, 0, $exp, 4 - $p.Exponent.Length, $p.Exponent.Length)

$pre = [byte[]](0x00,0x00,0x22,0x22)
$sha = [System.Security.Cryptography.SHA256]::Create()
try {
    $h = $sha.ComputeHash($pre + $exp + $p.Modulus)[0..15]
} finally {
    $sha.Dispose()
}
$url = "https://ftpm.amd.com/pki/aia/" + (($h | ForEach-Object { $_.ToString('X2') }) -join '')
$url

try {
    $r = Invoke-WebRequest -Uri $url -TimeoutSec 20
    "HTTP $($r.StatusCode): response received ($($r.RawContentLength) bytes)"
} catch {
    $status = if ($_.Exception.Response) { [int]$_.Exception.Response.StatusCode } else { $null }
    if ($status -eq 404) {
        "HTTP 404: no object exists at this derived URL"
    } elseif ($null -ne $status) {
        "HTTP ${status}: inconclusive response"
    } else {
        "Request failed: inconclusive ($($_.Exception.Message))"
    }
}
```

Reading the result:

- HTTP 200: your key has a pre-registered AMD cert. Chain anchors are public (`PRG-RPL` intermediate and `AMDTPM` roots at the same host, all verified live 2026-08-21). **[A]**
- HTTP 404: no cert exists for your key. Attestation that validates the EK cert will fail. Cycle back to your previous identity, because that one had a cert.

> [!CAUTION]
> A response from this derived URL does not by itself prove that a certificate matches the active EK or builds to a trusted root. A 404 is also inconclusive while the suffix construction remains **[S]**. Prefer the certificates Windows returns, validate their public key and chain, and do not clear or flash again based only on this script.

Caveats: this scheme matches the documented AMD URL format for default-template RSA EKs. If you get 404 but `ManufacturerCertificates` shows a valid-looking cert, trust the cert and note the discrepancy. ECC EKs use a longer hash form; the PowerShell above covers the RSA case only.

---

## Certificate retrieval and trust

Firmware TPMs may need network access to retrieve manufacturer certificates during provisioning. Microsoft's current Autopilot requirements list `https://ftpm.amd.com/pki/aia` for AMD and `https://ekop.intel.com/ekcertservice` for Intel. The same documentation says discrete TPM devices normally ship with the needed certificates already installed. **[A]** [Windows Autopilot requirements](https://learn.microsoft.com/en-us/autopilot/requirements)

Microsoft documents those endpoints for certificate retrieval. It does not describe them as public certificate-minting APIs. An HTTP response alone does not prove that the returned certificate matches the active EK, chains to a trusted root, is within its validity period, or satisfies a particular relying party. Microsoft's EK-certificate attestation model separately validates the EK certificate chain against administrator-approved intermediate and root certificates. **[A]** [Microsoft TPM key attestation](https://learn.microsoft.com/en-us/windows-server/identity/ad-ds/manage/component-updates/tpm-key-attestation)

Intel's current public material needs platform context. Microsoft still lists EKOP as an Intel firmware-TPM dependency. Intel documents that CSME 15 and later use an On-Die Certificate Authority architecture. **[A]** Intel employee `liranper` stated that 11th-generation-and-later PTT EKs use an embedded intermediate chain instead of the older EKOP provisioning path. **[C]** Inspect the actual certificate and embedded chain instead of deciding trust from one endpoint test. [Intel CSME security white paper](https://www.intel.com/content/dam/www/public/us/en/security-advisory/documents/intel-csme-security-white-paper.pdf) and [Intel employee's PTT certificate-chain explanation](https://community.intel.com/t5/Mobile-and-Desktop-Processors/How-to-verify-an-Intel-PTT-endorsement-key-certificate/m-p/1613959)

Keep these results distinct in your evidence:

- **EKpub hash changed:** the public-key representation returned by Windows changed.
- **EK certificate changed:** compare the leaf certificate's serial, thumbprint, issuer, validity dates, and public key.
- **Certificate matches the active EK:** the certificate's public key equals the public key Windows returned for the current EK.
- **Chain validates:** the leaf builds through the expected intermediate certificates to an approved root.
- **Attestation succeeds:** a specific relying party accepted that chain and the rest of its policy. This cannot be inferred from the preceding checks alone.

---

## Intel: Z790 vs Z790-era method vs Z890

- Z790 generation: the Flash BIOS Button rewrite method is documented separately in `guides/tpm-spoofing/tpm-spoofing.md`. It was tested on MSI Z790. Treat it as generation-specific: that platform wrote the whole SPI chip including the ME data region.
- Z890 / Arrow Lake (current Intel gen): **no verified user-accessible rotation exists as of 2026-08-21.** Verified dead ends, so you do not waste time or brick anything:
  - Clear TPM regenerates storage keys only. The endorsement seed and EK survive by design. **[A]**
  - Flash BIOS Button does not rewrite the CSME data region on this generation. Field measurement on MSI MEG Z890 ACE showed the EK unchanged. ASUS documents the same region-scoped behavior for its 800-series boards. **[C]+[A]**
  - The ME firmware tool ships code-only payloads. Byte analysis of three MSI flashback files confirms no PTT state inside. **[A]**
  - The only documented fresh-EK-with-cert mechanism is Intel-triggered TCB recovery with a firmware security version bump (CSME white paper 631900, section 5.3). There is no user knob for it. **[A]**
- A single forum post claims flashing lowest-then-highest BIOS rotates EK on "most Gigabyte boards" for Intel 12th through 15th gen. Single source, no artifacts, contradicts the Z890 measurements above. **[S]** Do not plan around it.

> [!WARNING]
> The lowest-then-highest BIOS flash procedure is **[S]** and untested. Do not risk a firmware downgrade based on that report.

---

## Evidence ledger

Primary sources consulted live during research (2026-08-21):

| Item | Source | Grade |
|---|---|---|
| AMD cert server 404 for fabricated keys, stable serve for real keys | Live tests against `ftpm.amd.com/pki/aia` | [A] |
| Chain anchors PRG-RPL + AMDTPM root generations | Same host, parsed DER | [A] |
| Pluton Global Factory ICA reachable (ECDSA P-384, 2023-2039) | `ftpm.amd.com/hsp/ica/` | [A] |
| Microsoft Pluton Root CA 2021 reachable | microsoft.com/pkiops | [A] |
| Pluton dropped as AMD TPM from 2026 silicon | learn.microsoft.com pluton-as-tpm | [A] |
| CVE-2025-2884 = TCG reference code OOB read, AMD fTPM affected, PI fix table per platform | amd.com SB-4011 | [A] |
| TPM-B FW changelog wording example (F67i) | Gigabyte support page screenshot, thread artifact | [C] |
| AMI reset prompt text | Thread screenshot l4d4SXX | [C] |
| Deterministic two-set cycling | X570 Aorus Elite owner report, UC thread 720900 | [C] |
| Board-by-board results table | UC threads 720900 pages 1-6, cross-read 2026-08-21 | [CC]-weighted [C] list above |
| Windows fetches EK cert from ftpm.amd.com at provision | Microsoft Autopilot endpoint docs + two independent tutorials | [A]+[CC] |
| Anti-cheat stacks validate the EK cert chain (fail closed when missing) | Intel Community threads on RICOCHET attestation failures, HP OMEN Pluton defect case | [CC] |
| Pluton toggle infinite-EK claim | UC thread 749560 (2026-04-24), evidence album shows duplicate hashes | [S] |

Known open questions (nobody has answered these publicly):

1. Does the Pluton toggle produce a fresh chain-valid certificate? Needs paired hardware measurement.
2. Does a TPM-B flash rotation leave the NEW key with a server-registered cert on AM5? Needs one paired test on any listed board. The check script above answers it in five minutes.
3. Does the ASP fTPM EK derivation include the fTPM firmware version, or something coarser? Explains the two-set behavior either way, but the exact input set is undocumented.

> [!NOTE]
> **Diagnostic limit:** the script can probe one derived URL, but it cannot answer question 2 by itself while the AMD suffix construction remains **[S]**. A useful result also needs a paired EK public key, the returned certificate, a public-key match, and chain validation.

Boundary note: this page records what verifiably changes the identity and how to check it safely. It does not rank paths by anti-cheat acceptance.

---

## Sources

- [TCG TPM 2.0 Library specification index](https://trustedcomputinggroup.org/resource/tpm-library-specification/)
- [TCG TPM 2.0 Library Part 1: Architecture, version 185](https://trustedcomputinggroup.org/wp-content/uploads/Trusted-Platform-Module-2.0-Library-Part-1-Architecture_Version-185_pub.pdf)
- [TCG EK Credential Profile for TPM 2.0, version 2.7](https://trustedcomputinggroup.org/wp-content/uploads/TCG-EK-Credential-Profile-for-TPM-Family-2.0-Level-0-Version-2.7_Pub.pdf)
- [Microsoft: Troubleshoot and clear the TPM](https://learn.microsoft.com/en-us/windows/security/hardware-security/tpm/initialize-and-configure-ownership-of-the-tpm)
- [Microsoft: Get-TpmEndorsementKeyInfo](https://learn.microsoft.com/en-us/powershell/module/trustedplatformmodule/get-tpmendorsementkeyinfo?view=windowsserver2025-ps)
- [Microsoft: tpmtool](https://learn.microsoft.com/en-us/windows-server/administration/windows-commands/tpmtool)
- [Microsoft: How Windows uses the TPM](https://learn.microsoft.com/en-us/windows/security/hardware-security/tpm/how-windows-uses-the-tpm)
- [Microsoft: Windows Autopilot requirements](https://learn.microsoft.com/en-us/autopilot/requirements)
- [Microsoft: TPM key attestation](https://learn.microsoft.com/en-us/windows-server/identity/ad-ds/manage/component-updates/tpm-key-attestation)
- [Microsoft .NET: AsnEncodedData](https://learn.microsoft.com/en-us/dotnet/api/system.security.cryptography.asnencodeddata)
- [Microsoft .NET: RSA.ImportSubjectPublicKeyInfo](https://learn.microsoft.com/en-us/dotnet/api/system.security.cryptography.rsa.importsubjectpublickeyinfo)
- [Microsoft .NET: RSA.ImportRSAPublicKey](https://learn.microsoft.com/en-us/dotnet/api/system.security.cryptography.rsa.importrsapublickey)
- [Microsoft: Pluton as TPM](https://learn.microsoft.com/en-us/windows/security/hardware-security/pluton/pluton-as-tpm)
- [Intel: CSME security white paper](https://www.intel.com/content/dam/www/public/us/en/security-advisory/documents/intel-csme-security-white-paper.pdf)
- [Intel employee: PTT endorsement certificate chain explanation](https://community.intel.com/t5/Mobile-and-Desktop-Processors/How-to-verify-an-Intel-PTT-endorsement-key-certificate/m-p/1613959)
