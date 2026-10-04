#requires -Version 7.0
[CmdletBinding()]
param()

$RustRoot = Split-Path -Parent $PSScriptRoot
Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
$PSNativeCommandUseErrorActionPreference = $false

function Invoke-Checked([string]$Exe, [string[]]$Arguments) {
    & $Exe @Arguments
    if ($LASTEXITCODE -ne 0) {
        throw "$Exe $($Arguments -join ' ') failed (exit $LASTEXITCODE)."
    }
}

function Get-BuildTools {
    $vswhere = Join-Path ${env:ProgramFiles(x86)} 'Microsoft Visual Studio/Installer/vswhere.exe'
    if (-not (Test-Path -LiteralPath $vswhere)) { throw "vswhere.exe not found: $vswhere" }
    $paths = @(Invoke-Checked $vswhere @('-latest', '-products', '*', '-requires',
        'Microsoft.VisualStudio.Component.VC.Tools.x86.x64', '-find',
        'VC/Tools/MSVC/**/bin/Hostx64/x64/dumpbin.exe'))
    if ($paths.Count -eq 0) { throw 'dumpbin.exe not found via vswhere; install the VS x64 C++ build tools.' }
    $dumpbin = $paths | Get-Item | Sort-Object {
        [version]$_.Directory.Parent.Parent.Parent.Name
    } -Descending | Select-Object -First 1
    $kits = Get-ItemPropertyValue -LiteralPath 'HKLM:\SOFTWARE\Microsoft\Windows Kits\Installed Roots' -Name KitsRoot10
    $manifests = @(Get-ChildItem -Path (Join-Path $kits 'bin/10.*/x64/mt.exe') -File)
    if ($manifests.Count -eq 0) { throw 'mt.exe not found; install the Windows 10/11 SDK.' }
    $mt = $manifests | Sort-Object { [version]$_.Directory.Parent.Name } -Descending | Select-Object -First 1
    return @{ Dumpbin = $dumpbin.FullName; Mt = $mt.FullName }
}

function Assert-Imports([string]$Dumpbin, [string]$Binary) {
    $output = @(Invoke-Checked $Dumpbin @('/nologo', '/dependents', $Binary))
    $imports = @($output | ForEach-Object {
        if ($_ -match '^\s+([^\s]+\.dll)\s*$') { $Matches[1] }
    })
    if ($imports.Count -eq 0) { throw 'dumpbin returned no DLL imports; cannot verify the static CRT.' }
    $allowed = @('KERNEL32', 'USER32', 'GDI32', 'ADVAPI32', 'OLE32', 'OLEAUT32',
        'SHELL32', 'COMCTL32', 'UXTHEME', 'SETUPAPI', 'CFGMGR32', 'IPHLPAPI',
        'WS2_32', 'WINHTTP', 'BCRYPT', 'NCRYPT', 'CRYPT32', 'WEVTAPI', 'NTDLL', 'COMBASE',
        'DWMAPI', 'bcryptprimitives')
    $rejected = @($imports | Where-Object {
        $name = $_ -replace '\.dll$', ''
        $name -notin $allowed -and $name -notmatch '^api-ms-win-core-[a-z0-9-]+$'
    })
    if ($rejected.Count) { throw "Unapproved DLL imports: $($rejected -join ', ')" }
    Write-Host "Import allow-list passed: $($imports -join ', ')"
}

function Assert-Manifest([string]$Mt, [string]$Binary) {
    $temp = [System.IO.Path]::GetTempFileName()
    try {
        Invoke-Checked $Mt @('-nologo', "-inputresource:$Binary;#1", "-out:$temp")
        $settings = [System.Xml.XmlReaderSettings]::new()
        $settings.DtdProcessing = [System.Xml.DtdProcessing]::Prohibit
        $reader = [System.Xml.XmlReader]::Create($temp, $settings)
        try {
            $manifest = [System.Xml.XmlDocument]::new()
            $manifest.Load($reader)
        } finally { $reader.Dispose() }
        $admin = $manifest.SelectSingleNode("//*[local-name()='requestedExecutionLevel' and @level='requireAdministrator']")
        $dpi = $manifest.SelectSingleNode("//*[local-name()='dpiAwareness' and namespace-uri()='http://schemas.microsoft.com/SMI/2016/WindowsSettings']")
        $controls = $manifest.SelectSingleNode("//*[local-name()='dependentAssembly']/*[local-name()='assemblyIdentity' and @name='Microsoft.Windows.Common-Controls' and @version='6.0.0.0']")
        if ($null -eq $admin) { throw 'Embedded manifest lacks requireAdministrator.' }
        if ($null -eq $dpi -or 'PerMonitorV2' -cnotin @($dpi.InnerText.Split(',').Trim())) {
            throw 'Embedded manifest lacks PerMonitorV2.'
        }
        if ($null -eq $controls) { throw 'Embedded manifest lacks Common Controls 6.0.' }
        Write-Host 'Embedded manifest passed: requireAdministrator, PerMonitorV2, Common Controls 6.0.'
    } finally { Remove-Item -LiteralPath $temp }
}

Push-Location $RustRoot
try {
    if (Test-Path Env:RUSTFLAGS) { throw 'RUSTFLAGS is set. Remove it: it replaces the configured +crt-static rustflags.' }
    if (Test-Path Env:CARGO_ENCODED_RUSTFLAGS) { throw 'CARGO_ENCODED_RUSTFLAGS is set; remove this rustflags override.' }
    Invoke-Checked cargo @('fmt', '--check')
    Invoke-Checked cargo @('clippy', '--locked', '--all-targets', '--', '-D', 'warnings')
    Invoke-Checked cargo @('test', '--locked')
    Invoke-Checked cargo @('build', '--release', '--locked')
    $metadata = (Invoke-Checked cargo @('metadata', '--locked', '--no-deps', '--format-version', '1')) | ConvertFrom-Json
    $version = ($metadata.packages | Where-Object name -eq 'hwidchecker').version
    if ($version -notmatch '^\d+\.\d+\.\d+$') { throw "Unsupported app version: $version" }
    $target = $metadata.target_directory
    if ($env:CARGO_BUILD_TARGET) { $target = Join-Path $target $env:CARGO_BUILD_TARGET }
    $binary = Join-Path $target 'release/HWIDChecker.exe'
    if (-not (Test-Path -LiteralPath $binary)) { throw "Release executable not found: $binary" }
    $info = (Get-Item -LiteralPath $binary).VersionInfo
    foreach ($field in @('FileVersion', 'ProductVersion')) {
        if ($info.$field -cne $version) {
            throw "Executable $field must equal $version (Cargo version); got $($info.$field)."
        }
    }
    foreach ($field in @('File', 'Product')) {
        $numeric = '{0}.{1}.{2}.{3}' -f $info."${field}MajorPart", $info."${field}MinorPart",
            $info."${field}BuildPart", $info."${field}PrivatePart"
        if ($numeric -cne "$version.0") {
            throw "Executable ${field}Version numeric parts must equal $version.0; got $numeric."
        }
    }
    Write-Host "Executable resource versions match Cargo $version."
    $tools = Get-BuildTools
    Assert-Imports $tools.Dumpbin $binary
    Assert-Manifest $tools.Mt $binary
    Write-Host 'All base checks passed.'
    exit 0
} catch {
    Write-Error $_.Exception.Message -ErrorAction Continue
    exit 1
} finally {
    Pop-Location
}
