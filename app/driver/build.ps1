#requires -Version 7.0
[CmdletBinding()]
param(
    [ValidateSet('Debug', 'Release')][string]$Configuration = 'Debug',
    [ValidateSet('Unsigned', 'Test', 'Whcp')][string]$Signing = 'Unsigned',
    [string]$CertificateThumbprint,
    [switch]$MachineCertificate,
    [string]$SignedPackage,
    [string]$KitVersion,
    [string]$Inf2CatOs = '10_VB_X64,10_NI_X64,10_GE_X64',
    [switch]$ControllerPackage,
    [switch]$CheckPrerequisites
)
Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
$PSNativeCommandUseErrorActionPreference = $false
. (Join-Path $PSScriptRoot 'scripts/tools.ps1')

function Get-KitTool([string]$Name) {
    foreach ($relative in @("bin/$KitVersion/x64/$Name", "bin/$KitVersion/x86/$Name", "Tools/$KitVersion/x64/$Name")) {
        $candidate = Join-Path $kits $relative
        if (Test-Path -LiteralPath $candidate) { return $candidate }
    }
    throw "$Name is missing from WDK $KitVersion. Install the matching WDK tools."
}

try {
    $kits = Get-ItemPropertyValue -LiteralPath 'HKLM:\SOFTWARE\Microsoft\Windows Kits\Installed Roots' -Name KitsRoot10
    $versions = @(Get-ChildItem -LiteralPath (Join-Path $kits 'Include') -Directory |
        Where-Object { $_.Name -match '^10\.0\.\d+\.\d+$' -and
            (Test-Path -LiteralPath (Join-Path $_.FullName 'km/ntddk.h')) -and
            (Test-Path -LiteralPath (Join-Path $_.FullName 'um/Windows.h')) -and
            (Test-Path -LiteralPath (Join-Path $kits "Lib/$($_.Name)/km/x64/ntoskrnl.lib")) } |
        Sort-Object { [version]$_.Name } -Descending)
    if (-not $KitVersion -and $versions.Count -gt 0) { $KitVersion = $versions[0].Name }
    if (-not $KitVersion -or $KitVersion -notin @($versions | ForEach-Object { $_.Name })) {
        $sdkVersions = @(Get-ChildItem -LiteralPath (Join-Path $kits 'Include') -Directory |
            Where-Object { $_.Name -match '^10\.0\.\d+\.\d+$' } | Select-Object -ExpandProperty Name)
        Write-Host 'WDK absent: no matching kernel headers/libraries. Nothing was built, signed, installed or loaded.'
        Write-Host "Installed SDK versions: $($sdkVersions -join ', ')."
        Write-Host 'Install Visual Studio 2026 Build Tools with Desktop development with C++ (MSVC x64/x86).'
        Write-Host 'Visual Studio Installer > Modify > Individual components > Windows Driver Kit (VSIX).'
        Write-Host 'Install the Windows 11 WDK and Windows SDK with IDENTICAL complete build versions.'
        Write-Host 'For SDK 10.0.26100.0, install WDK 10.0.26100.0, or upgrade BOTH kits to one matching release.'
        Write-Host 'Official installer and matching-kit guidance: https://learn.microsoft.com/en-us/windows-hardware/drivers/download-the-wdk'
        exit 2
    }
    $visualStudio = Get-VisualStudio
    $msbuild = Join-Path $visualStudio 'MSBuild/Current/Bin/MSBuild.exe'
    if (-not (Test-Path -LiteralPath $msbuild)) { throw 'MSBuild is missing from Visual Studio Build Tools.' }
    $stampinf = Get-KitTool 'stampinf.exe'
    $infverif = Get-KitTool 'InfVerif.exe'
    $inf2cat = Get-KitTool 'Inf2Cat.exe'
    $signtool = Get-KitTool 'signtool.exe'
    if ($CheckPrerequisites) { Write-Host "Driver prerequisites found: VS=$visualStudio; SDK/WDK=$KitVersion."; exit 0 }
    if ($Signing -eq 'Test' -and $CertificateThumbprint -notmatch '^[0-9a-fA-F]{40}$') {
        throw 'Test signing requires -CertificateThumbprint for an existing owner-provisioned code-signing certificate.'
    }
    if ($Signing -eq 'Whcp' -and -not $SignedPackage) {
        throw 'WHCP mode requires -SignedPackage pointing to the Microsoft-signed package returned AFTER HLK submission. An .hlkx is not a signed driver.'
    }
    Enter-DriverHostEnvironment $visualStudio
    Invoke-Checked $msbuild @((Join-Path $PSScriptRoot 'HWIDReader.vcxproj'), '/t:Rebuild', '/m',
        "/p:Configuration=$Configuration", '/p:Platform=x64', "/p:WindowsTargetPlatformVersion=$KitVersion",
        '/p:RunCodeAnalysis=true', '/p:SignMode=Off', '/p:EnableInf2cat=false')
    $package = Join-Path $PSScriptRoot "out/packages/$([guid]::NewGuid().ToString('N'))"
    New-Item -ItemType Directory -Path $package -Force | Out-Null
    $infName = if ($ControllerPackage) { 'HWIDReaderControllers.inf' } else { 'HWIDReader.inf' }
    $infSource = if ($ControllerPackage) { "lab/$infName" } else { $infName }
    Copy-Item -LiteralPath (Join-Path $PSScriptRoot "out/$Configuration/HWIDReader.sys") -Destination $package
    Copy-Item -LiteralPath (Join-Path $PSScriptRoot $infSource) -Destination $package
    $inf = Join-Path $package $infName
    Invoke-Checked $stampinf @('-f', $inf, '-d', (Get-Date -Format 'MM/dd/yyyy'), '-v', '0.1.0.0', '-a', 'amd64', '-k', '1.15')
    Invoke-Checked $infverif @('/u', $inf)
    $sys = Join-Path $package 'HWIDReader.sys'
    if ($Signing -eq 'Test') {
        $signArguments = @('sign', '/fd', 'SHA256', '/sha1', $CertificateThumbprint, '/s', 'My')
        if ($MachineCertificate) { $signArguments += '/sm' }
        Invoke-Checked $signtool ($signArguments + @($sys))
    }
    # Catalog creation follows embedded signing, so hashes describe the final .sys.
    Invoke-Checked $inf2cat @("/driver:$package", "/os:$Inf2CatOs", '/uselocaltime')
    $catalog = Join-Path $package ($infName -replace '\.inf$', '.cat')
    if (-not (Test-Path -LiteralPath $catalog)) { throw "Expected catalog missing: $catalog" }
    if ($Signing -eq 'Test') {
        Invoke-Checked $signtool ($signArguments + @($catalog))
        Invoke-Checked $signtool @('verify', '/pa', '/v', $catalog)
        Invoke-Checked $signtool @('verify', '/pa', '/v', '/c', $catalog, $sys)
        Write-Host 'Test package only. Trust provisioning is a separate owner action on an isolated lab machine.'
    } elseif ($Signing -eq 'Whcp') {
        $signedRoot = (Resolve-Path -LiteralPath $SignedPackage).Path
        $signedCat = Join-Path $signedRoot (Split-Path $catalog -Leaf)
        $signedSys = Join-Path $signedRoot 'HWIDReader.sys'
        $signedInf = Join-Path $signedRoot $infName
        foreach ($file in @($signedSys, $signedInf, $signedCat)) {
            if (-not (Test-Path -LiteralPath $file -PathType Leaf)) { throw "WHCP output file missing: $file" }
        }
        Invoke-Checked $signtool @('verify', '/kp', '/v', $signedCat)
        Invoke-Checked $signtool @('verify', '/kp', '/v', '/c', $signedCat, $signedSys)
        Invoke-Checked $signtool @('verify', '/kp', '/v', '/c', $signedCat, $signedInf)
        # This verifies returned package signatures/membership, not certification,
        # provenance or equivalence to this rebuild. VALIDATION.md has that gate.
        Write-Host "WHCP returned-package signatures checked: $signedRoot. Owner must match its HLK submission/build manifest."
    }
    Write-Host "Local package: $package (Signing=$Signing). Driver Verifier and hardware/HLK gates remain pending."
    exit 0
} catch {
    Write-Error $_.Exception.Message -ErrorAction Continue
    exit 1
}
