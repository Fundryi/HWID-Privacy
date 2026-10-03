# fTPM Identity Reset Guide (AMD AM5 + Intel status)

> [!NOTE]
> **TL;DR:** Rotates the AMD fTPM endorsement identity by cycling BIOS versions that carry an fTPM firmware (TPM-B) change. Intel Z890 currently has no verified user-accessible rotation.
> Who reads it: Windows attestation, BitLocker, and any relying party that validates the EK certificate chain.
> **Status:** identity change confirmed on the boards listed below **[C]**. Certificate validity after rotation is untested in public.
> This guide changes TPM state. Read all of it before you touch anything.
> If you use BitLocker or any drive encryption, suspend it or save your recovery key first. A TPM reset can lock you out of your drives.
> BIOS flashing has real risk. Use only the exact BIOS file for your exact board revision. A power cut during a flash can kill the board.

Evidence grades appear inline. See [How to read these guides](../getting-started/getting-started.md#how-to-read-these-guides). **[C]** confirmed first hand by a named user with details, **[A]** verified against a cited primary source (or agent-verified live during research, 2026-08-21), **[CC]** community consensus, many reports, **[S]** single unverified claim. Untested steps are marked.

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

## Before you reset

Read the [TPM identity terms](../tpm-spoofing/tpm-spoofing.md#tpm-identity-and-terminology), [clear precautions](../tpm-spoofing/tpm-spoofing.md#what-clearing-the-tpm-changes), and [firmware-update limits](../tpm-spoofing/tpm-spoofing.md#firmware-updates-and-ek-continuity) first. Complete those precautions and capture the baseline below before any clear, firmware flash, or switch between fTPM, dTPM, and Pluton. See also the [safety checklist](../getting-started/getting-started.md#safety-checklist).

## How the AMD fTPM identity actually works

Three facts drive everything below.

1. The AMD fTPM endorsement key (EK) is not random per reset. Reports describe two fixed identities per CPU/firmware pair: a user on a Gigabyte X570 Aorus Elite downgraded and got his original hashes back exactly, then upgraded and got the same second set again. The explanation "derived from secrets inside your CPU plus the running fTPM firmware version" fits all reports, but no public primary source documents the exact derivation inputs; treat it as the leading hypothesis. **[CC]** for the observation, **[S]** for the derivation formula.
   - A user on a Gigabyte X570 Aorus Elite downgraded and got his original hashes back exactly, then upgraded and got the same second set again. Two fixed identities, not infinite ones. **[C]**
   - Multiple users flashed BIOS versions that did not carry an fTPM firmware change. Their identity stayed the same. **[CC]**
2. Wiping TPM state alone does not make a new EK. The AMI prompt ("Press Y to reset fTPM") rebuilds NV structures. It does not reseed the endorsement key by itself. **[A]** from primary sources, confirmed by reports where Y was pressed with no firmware change and hashes stayed identical.
3. Windows requires the EK certificate retrieval endpoint `https://ftpm.amd.com/pki/aia/` to be reachable so firmware-TPM certificates can be fetched on first use (the community probe used `https://ftpm.amd.com/pki/aia/<hash-of-EKpub>`-shaped URLs; the exact suffix construction is unverified). During live testing the server returned HTTP 404 for keys it did not know and served stable certs for real chips. Whether registration is static per-CPU or dynamic is not documented. **[A]** for the endpoint requirement, **[S]** for the registration and suffix internals.

<details><summary>Older info (outdated)</summary>

3. Windows fetches the EK certificate online from `https://ftpm.amd.com/pki/aia/<hash-of-EKpub>` at provisioning time and stores it in TPM NV. That server serves pre-registered certificates only. During live testing it returned HTTP 404 for keys it does not know and served stable certs for real chips. It does not mint certificates for arbitrary new keys on demand. **[A]** The historical Pluton ICA probe used `ftpm.amd.com/hsp/ica/`.

</details>

Consequence: a rotation path is only useful if your new EK also gets a valid certificate. If the server has never seen your new key hash, you get no manufacturer cert. Attestation-based checks then fail instead of pass. Check this after every attempt (section below).

> [!IMPORTANT]
> Primary sources establish that a normal clear does not change the EPS and that AMD firmware TPMs need access to AMD's certificate-retrieval endpoint. They do not document AMD's EK derivation inputs, the endpoint's per-key suffix, or whether certificates are pre-registered rather than created on request. Treat those AMD-specific explanations above as community or live-service observations, not **[A]** facts. The TCG field-upgrade requirement also says the original manufacturer-certified EK must remain reproducible for the same template until `TPM2_ChangeEPS`. **[A]** [TCG architecture](https://trustedcomputinggroup.org/wp-content/uploads/Trusted-Platform-Module-2.0-Library-Part-1-Architecture_Version-185_pub.pdf) and [Windows Autopilot requirements](https://learn.microsoft.com/en-us/autopilot/requirements)

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

If you can boot Linux, this reads the persistent objects and NV storage directly, bypassing the Windows cache:

```bash
tpm2_getcap handles-persistent
tpm2_readpublic -c 0x81010001 -f pem -o ek.pem        # then sha256 over the DER
tpm2_nvread 0x01c00002 -C o -o ekcert.der             # RSA EK cert if populated
openssl x509 -inform der -in ekcert.der -text -noout  # serial, NotBefore, issuer
```

These commands read whatever object currently sits at handle `0x81010001` and index `0x01c00002`. They do not prove that object is the genuine, endorsement-hierarchy EK: see [the persistent-handle trap](#check-your-certificate-after-any-rotation). A serial-and-date pair is a certificate observation, not a trust test.

Also run an MMIO-based checker if available. Community reports say cached readings and MMIO readings can disagree. **[CC]**

## Windows views and HWIDChecker baseline

Use the commands and interpretation in [Inspect the TPM in Windows](../tpm-spoofing/tpm-spoofing.md#inspect-the-tpm-in-windows) and [Verify with HWIDChecker.exe](../tpm-spoofing/tpm-spoofing.md#verify-with-hwidcheckerexe) before and after each attempt. Preserve the raw PowerShell certificate collections. HWIDChecker is a convenient paired view, not an independent measurement or a complete certificate inventory.

## Method A: TPM-B firmware flash cycle

This is the only AMD rotation path with multiple independent first-hand reports. It works because some BIOS updates ship a new version of the fTPM firmware itself ("TPM-B FW"). When the board boots with a different fTPM firmware version, the AMI screen appears and offers to reset fTPM. Users report that accepting it brings the TPM up with a different EK. A TPM-B changelog entry or the AMI Y prompt alone does not prove a new endorsement identity; the paired key and certificate check in the next sections is the proof.

> [!WARNING]
> **Certificate status after a TPM-B rotation is not publicly proven.** AMD's public `ftpm.amd.com/pki/aia/<hash>` endpoint is a hash-addressed certificate lookup: the request does not contain the full EK public key or a certificate request, so it cannot create a certificate for an unknown EK from that request alone. **[A]** Public sources do not show when AMD registers certificates for keys produced by later fTPM firmware. Some firmware updates restore attestation, but real systems have also reported empty `ManufacturerCertificates` collections and HTTP 404 for their active EK. **[S]** Treat a changed EK as untrusted until its leaf certificate matches the active EK, its chain validates to an accepted AMD root, and Windows plus the relying party both pass attestation. If any check fails, the rotation produced an attestation failure, not a clean identity.

**Status**: reported identity change on the boards listed below. Certificate validity after rotation is UNTESTED in public. See the warning in the next paragraph before relying on the new state.

### Step 0: Prepare (do not skip)

1. Suspend BitLocker, or save recovery keys for every encrypted drive. Conditions: if you skip this and press Y later, encrypted drives will demand the recovery key.
2. Plug in a USB stick formatted FAT32.
3. Download BIOS files ONLY from your board's official support page. Match your exact model AND board revision (rev 1.x vs rev 2.x are different files).
4. Note your current BIOS version (BIOS main screen, or `Get-ComputerInfo | Select BiosSMBIOSBIOSVersion`).
5. Unzip the BIOS file onto the USB root. The file name must match your board.

### Step 1: Pick the right BIOS pair

Open your board's support page BIOS list. Look for a changelog entry like this (real example, Gigabyte B450M DS3H F67, 2025-10-29):

> Update TPM-B FW for Raven2/ Picasso, Cezanne, Vermeer/ Matisse & Renoir CPUs
> Fix TPM2.0's out-of-bounds read vulnerability (CVE-2025-2884)

Any version jump that includes a TPM-B FW update is a candidate. You need TWO versions: your current one and one with the TPM-B update. The rotation comes from moving between them. Moving again moves you back.

AMD shipped the CVE-2025-2884 fTPM fixes to OEMs between 2024-09 and 2025-04 depending on platform (AMD-SB-4011). Which vendor BIOSes carry a TPM-B change is board- and platform-specific: check your exact board's changelog, not the date. **[A]**

Vendors publish the TPM-B wording in their changelogs. Gigabyte AM4 boards use "Update TPM-B FW for Raven2/Picasso/Cezanne/Vermeer/Matisse & Renoir CPUs" (for example [B450M DS3H F67](https://www.gigabyte.com/Motherboard/B450M-DS3H-rev-1x/support), 2025-10-29). **[A]** [ASRock](https://www.asrock.com/support/faq.asp?id=548) documents affected AM4 boards moving from fTPM `3.*.0.*` to `3.*.2.*` **[A]**, and [MSI](https://www.msi.com/index.php/blog/how-to-enable-secure-boot-and-tpm-2-0-on-msi-am4-motherboards) documents the raise to `3.94.2.5` **[A]**. [AMD's attestation matrix](https://www.amd.com/en/resources/support-articles/faqs/pa-420.html) says ASP fTPM `3.*.0.*` fails attestation while `3.*.2.*` and `6.*.*.*` pass for the affected Ryzen 1000-5000 range, which is why these updates exist. **[A]**

### Step 2: Flash

1. Reboot into BIOS (Del on most boards). Open Q-Flash (F8 on Gigabyte, or from the BIOS menu).
2. Select the BIOS file from the USB stick. Confirm. Do not touch power during the flash.
3. On reboot, watch for this screen:

   > American Megatrends ... New CPU installed, fTPM/PSP NV corrupted or fTPM/PSP NV structure changed.
   > Press Y to reset fTPM ... Press N to keep previous fTPM record ...

4. Press Y. (This is the point of no return for BitLocker keys. Step 0 must be done.)
5. Boot into Windows. Run the paired measurement from the section above. Compare EK hash, cert serial, NotBefore.

To get a third state later, flash back toward your old version and accept the prompt again. Expect to land back on your original identity; that is what the two-state observation predicts. **[C]** on X570, untested elsewhere.

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
Gigabyte B650M DS3H | F30 to F35 no change (that pair carries no TPM-B change); a BIOS with a TPM-B entry is the right target | [C]
Gigabyte B650M D3HP | Lowest-to-highest without TPM-B pair, unchanged | [C]
Gigabyte B450M Gaming rev 1.x | Unchanged, no prompt | [C]
Gigabyte B650 Aorus Elite AX rev 1.2 | No luck | [C] relayed
ASRock B850M Steel Legend WiFi | No such option/update found | [C]

Pattern: the method is reported to work when your old-to-new pair actually contains an fTPM firmware version bump, and to fail otherwise. "Pure luck" in community words equals "check the changelog". **[CC]**

> [!NOTE]
> Row-level sources: most rows in this table come from one community discussion whose original thread (UC 720900) is no longer publicly accessible. The public remainder is a [separate one-page thread](https://www.unknowncheats.me/forum/anti-cheat-research/710632-ftpm-changing-flashing-bios.html) supporting only the B550M DS3H rev. 1.0 row, without BIOS versions or key hashes. Treat rows without an individual source as **[S]** reports, not independent confirmations.

### Failure modes and how to avoid them

- Wrong file for your board revision: brick risk. Match model and rev exactly.
- Downgrading too far on AM4: some newer CPUs cannot boot old AGESA versions. One 5700X3D owner could not post on old BIOS. If you have a late CPU, prefer the smallest step that still contains the TPM-B change.
- Some boards need an intermediate version when jumping across big ranges (one B450 owner had to pass through F32). If Q-Flash refuses a file, step through intermediate versions.
- Pressing N keeps the old state. Nothing rotates. You must press Y.
- Laptop OEMs may block rollback entirely (an HP case could not roll back at all).

## Method B: Pluton toggle (unverified)

On AM5 Gigabyte boards: Advanced -> Miscellaneous -> Trusted Platform Module. Options: Auto, Disabled, Enable dTPM, Enable ASP fTPM, Enable Pluton fTPM. ASUS, MSI and Supermicro expose the same choice under their own names. **[A]**

The claim from April 2026: switching to Pluton and back generates a completely fresh EK every time, while the ASP fTPM identity stays permanent. The displayed-key part has artifact evidence on one board **[C]**; the every-time and permanence parts remain **[S]**.

> [!WARNING]
> This procedure is **[S]** overall. The album evidence covers a displayed Pluton public-key change on one board, not a fresh certificate, a trusted chain, or repeatability across boards. Do not rely on it as an identity-rotation method.

Why we do not fully trust it yet:

- The poster's attached screenshot album ([UC 749560](https://www.unknowncheats.me/forum/anti-cheat-research/749560-infinite-tpm-identities-pluton-ftpm-amd-gigabyte.html), [album](https://imgur.com/a/NnNRKoK)) shows one AMD ASP identity and two Pluton identities with different public-key hashes, on one board (B850 Eagle). That is first-hand evidence of a displayed key change **[C]**, but only from one user and one board.
- His measurement was the cache-prone PowerShell reading; nobody posted a reboot-to-reboot sequence.
- Nobody in the thread verified a certificate came along with the new key. Valid Pluton chains exist (AMD Pluton Global Factory ICA and Microsoft Pluton Root CA 2021 are in [Microsoft's trusted TPM package](https://go.microsoft.com/fwlink/?linkid=2097925); see the [installation guidance](https://learn.microsoft.com/en-us/windows-server/security/guarded-fabric-shielded-vm/guarded-fabric-install-trusted-tpm-root-certificates)), but whether each re-activation mints a fresh chain-valid cert is publicly undocumented. **[A]**
- Microsoft states that on newly introduced 2026-and-later AMD and Qualcomm silicon, Pluton no longer serves as the TPM; 2025-and-earlier devices that shipped with Pluton as the TPM remain supported. **[A]**

If you try it anyway: full paired measurement, cert serial check, and the server-cert lookup from the next section. Report results with both screenshots.

## Method C: dTPM module

A discrete TPM module in the header carries its own factory EK and certificate. Mechanically this is a different chip's identity. Things to know:

- A faulty dTPM can fail attestation; [FACEIT documents this](https://support.faceit.com/hc/en-us/articles/20669555338268-TPM-attestation-failed) and suggests fTPM as the fix. **[A]** [Call of Duty's TPM requirements](https://support.activision.com/articles/trusted-platform-module-and-secure-boot/) explicitly list systems with a discrete TPM chip as supported. **[A]** Blanket "dTPM is flagged" rules are community reports, not vendor statements. **[S]**
- Modules are interchangeable commodities. Their certs come from Infineon, Nuvoton, ST and friends, which makes provenance look odd for a desktop build that should have firmware TPM.

Not recommended. Documented here so you do not waste money on it.

## What does NOT change the EK

Save yourself hours. These do nothing to the endorsement identity:

- Windows "Clear TPM", `Clear-Tpm`, or pressing Y at the AMI prompt when the fTPM firmware version did not change. **[A]** spec-level, plus field reports.
- CMOS clear, battery pull, BIOS settings reset. **[CC]**
- Reinstalling Windows. (One Intel user reported rotated keys after a fresh install plus BIOS flash, but that report is confounded with the flash itself.) **[S]**
- BIOS updates whose changelog has no TPM-B/fTPM entry. **[CC]**
- On Z890-class Intel: everything listed in the Intel section below.

> [!WARNING]
> Reinstalling Windows as an EK-rotation procedure is **[S]** and untested. The cited report also included a BIOS flash, so it does not isolate the reinstall as the cause.

## Check your certificate after any rotation

Run this after ANY identity change. It asks AMD's live server whether your current EK has a registered certificate.

> [!WARNING]
> **Diagnostic evidence limit:** the corrected script below was parser-tested with made-up keys; the full run on AMD hardware is not tested. Use PowerShell 7 or later as administrator. Do not treat a request failure as proof that a certificate is absent.

```powershell
# PowerShell 7+ as administrator. Diagnostic for RSA EKs only.
# PowerShell as admin. Best effort diagnostic for RSA EKs.
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

- HTTP 200: a certificate object exists at that identifier. Chain anchors are public (`PRG-RPL` intermediate and `AMDTPM` roots at the same host, verified live 2026-08-21). **[A]** Still verify that it is a leaf for the active EK and that the chain validates.
- HTTP 404: no object was returned for that identifier at that time. Recheck the URL derivation, service availability, and Windows provisioning before concluding that the key can never be certified. During an August 2026 `ftpm.amd.com` outage, some systems recovered automatically and others needed a retry-cache reset.
- Timeout or 5xx: service failure; inconclusive.
- `ManufacturerCertificates: {}`: Windows does not currently have a manufacturer certificate registered for the OS-visible EK. It does not distinguish "AMD has no object" from "retrieval, cache, or provisioning failed". Windows keeps endorsement state in the registry (`HKLM\SYSTEM\CurrentControlSet\Services\TPM\WMI\Endorsement`), so re-run provisioning with network access before deciding. **[A]**

Do not flash or clear again based only on this probe. An attestation result from the relying party that matters to you is the only final test. A missing certificate is primarily an availability risk on current evidence: RICOCHET restricts playlists, FACEIT and Battlefield 6 block play, Vanguard blocks launch, and Epic's Fortnite tournaments remove the player. None of those vendors documents a ban for the attestation failure itself. **[A]**

> [!CAUTION]
> A response from this derived URL does not by itself prove that a certificate matches the active EK or builds to a trusted root. A 404 is also inconclusive while the suffix construction remains **[S]**. Prefer the certificates Windows returns, validate their public key and chain, and do not clear or flash again based only on this script.
Caveats: this scheme matches the documented AMD URL format for default-template RSA EKs. If you get 404 but `ManufacturerCertificates` shows a valid-looking cert, trust the cert and note the discrepancy. ECC EKs use a longer hash form; the PowerShell above covers the RSA case only.

> [!WARNING]
> **The persistent-handle trap.** A changed hash at handle `0x81010001` plus a parseable certificate in NV index `0x01c00002` does not prove the endorsement identity changed. A community script ([UC 756450](https://www.unknowncheats.me/forum/pc-hardware/756450-tpm-spoofer-certificate-ftpm-amd.html), June 2026) does exactly this: it creates a new RSA key under the owner hierarchy, swaps it into `0x81010001`, and writes a locally signed, AMD-named EK leaf whose issuer certificate is discarded. The EPS and the genuine EK stay unchanged, and the leaf cannot chain to an AMD or Microsoft root. Verify the full chain (public key match, endorsement-hierarchy recreation, trusted root), not just "a different hash and a cert that parses". **[C]** for the code behavior; acceptance by any anti-cheat is unverified.
<details><summary>Older info (outdated): the original script</summary>

The first version of this check called `ExportParameters()` on `AsnEncodedData`, a method that does not exist there. It fails at runtime. Kept for reference:

```powershell
# PowerShell as admin. Best effort diagnostic for RSA EKs. (Broken: ExportParameters is not an AsnEncodedData method.)
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

</details>

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

## Evidence ledger

Primary sources consulted live during research (2026-08-21):

| Item | Source | Grade |
|---|---|---|
| AMD cert server 404 for fabricated keys, stable serve for real keys | Live tests against `ftpm.amd.com/pki/aia` | [A] |
| Chain anchors PRG-RPL + AMDTPM root generations | Same host, parsed DER | [A] |
| Pluton Global Factory ICA (ECDSA P-384, 2023-2039) | [Microsoft TrustedTpm package](https://go.microsoft.com/fwlink/?linkid=2097925) | [A] |
| Microsoft Pluton Root CA 2021 reachable | microsoft.com/pkiops | [A] |
| Pluton dropped as AMD TPM from 2026 silicon | learn.microsoft.com pluton-as-tpm | [A] |
| CVE-2025-2884 = TCG reference code OOB read, AMD fTPM affected, PI fix table per platform | amd.com SB-4011 | [A] |
| TPM-B FW changelog wording example (B450M DS3H F67) | [Gigabyte support page](https://www.gigabyte.com/Motherboard/B450M-DS3H-rev-1x/support), 2025-10-29 | [A] |
| AMI reset prompt text | Thread screenshot l4d4SXX | [C] |
| Deterministic two-set cycling | X570 Aorus Elite owner report, UC thread | [C] |
| Board-by-board results table | Community discussion (original thread 720900 no longer public; live remainder is UC 710632, one page) | [CC]-weighted [S] list above |
| Windows fetches EK cert from ftpm.amd.com at provision | Microsoft Autopilot endpoint docs + two independent tutorials | [A]+[CC] |
| Anti-cheat stacks use TPM attestation; documented product behavior varies (Call of Duty: failed attestation reduces playlists, not an instant ban) | Activision Season 04 update, FACEIT attestation article | [A] for the named products, unknown for others |
| dTPM presence alone is flagged | No primary source found; FACEIT documents only faulty-dTPM attestation failure, Call of Duty lists dTPM as supported | downgraded [S] |
| Pluton toggle fresh-EK claim | UC thread 749560 (2026-04-24); album shows two distinct Pluton hashes on one board | [C] for displayed hash change, [S] for the infinite-identity claim |

Known open questions (nobody has answered these publicly):

1. Does the Pluton toggle produce a fresh chain-valid certificate? Needs paired hardware measurement.
2. Does a TPM-B flash rotation leave the NEW key with a server-registered cert on AM5? Needs one paired test on any listed board: capture the EK public key and certificate before and after, then compare.
3. Does the ASP fTPM EK derivation include the fTPM firmware version, or something coarser? Explains the two-set behavior either way, but the exact input set is undocumented.

> [!NOTE]
> **Diagnostic limit:** the script can probe one derived URL, but it cannot answer question 2 by itself while the AMD suffix construction remains **[S]**. A useful result also needs a paired EK public key, the returned certificate, a public-key match, and chain validation.

Boundary note: this page records what verifiably changes the identity and how to check it safely. It does not rank paths by anti-cheat acceptance.

## Sources

- [Gigabyte B450M DS3H (rev. 1.x) support page](https://www.gigabyte.com/Motherboard/B450M-DS3H-rev-1x/support)
- [ASRock FAQ 548: TPMB fTPM updates](https://www.asrock.com/support/faq.asp?id=548)
- [MSI: Secure Boot and TPM 2.0 on AM4 motherboards](https://www.msi.com/index.php/blog/how-to-enable-secure-boot-and-tpm-2-0-on-msi-am4-motherboards)
- [AMD PA-420: fTPM attestation version matrix](https://www.amd.com/en/resources/support-articles/faqs/pa-420.html)
- [Microsoft: install trusted TPM root certificates (TrustedTpm package)](https://learn.microsoft.com/en-us/windows-server/security/guarded-fabric-shielded-vm/guarded-fabric-install-trusted-tpm-root-certificates)
- [FACEIT: TPM attestation failed](https://support.faceit.com/hc/en-us/articles/20669555338268-TPM-attestation-failed)
- [Activision: TPM 2.0 and Secure Boot requirements](https://support.activision.com/articles/trusted-platform-module-and-secure-boot/)
- [Call of Duty: RICOCHET Season 04 update](https://www.callofduty.com/au/en/blog/2026/06/call-of-duty-black-ops-7-warzone-ricochet-anti-cheat-season-04)
- [UnknownCheats: TPM spoofer + certificate script (persistent-handle example)](https://www.unknowncheats.me/forum/pc-hardware/756450-tpm-spoofer-certificate-ftpm-amd.html)
- [UnknownCheats: Infinite TPM identities with Pluton fTPM](https://www.unknowncheats.me/forum/anti-cheat-research/749560-infinite-tpm-identities-pluton-ftpm-amd-gigabyte.html)
- [UnknownCheats: ftpm changing after flashing bios?](https://www.unknowncheats.me/forum/anti-cheat-research/710632-ftpm-changing-flashing-bios.html)
- [tpm2-tools: AMD EK certificate retrieval issue](https://github.com/tpm2-software/tpm2-tools/issues/3158)
- [go-attestation: AMD URL construction](https://github.com/google/go-attestation/blob/master/attest/tpm.go)
- [Microsoft: TPM troubleshooting (multiple TPMs)](https://learn.microsoft.com/en-us/windows/security/hardware-security/tpm/initialize-and-configure-ownership-of-the-tpm)
- [Microsoft: Autopilot troubleshooting FAQ](https://learn.microsoft.com/en-sg/autopilot/troubleshooting-faq)

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
