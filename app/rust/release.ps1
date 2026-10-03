#requires -Version 7.0
[CmdletBinding()]
param()

Set-Location $PSScriptRoot
Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
$PSNativeCommandUseErrorActionPreference = $false

function Assert-DistPe([string]$Binary) {
    $bytes = [System.IO.File]::ReadAllBytes($Binary)
    if ($bytes.Length -lt 64 -or [BitConverter]::ToUInt16($bytes, 0) -ne 0x5A4D) {
        throw 'Dist executable lacks a valid MZ header.'
    }
    $offset = [BitConverter]::ToUInt32($bytes, 60)
    if ($offset -lt 64 -or $offset -gt $bytes.Length - 24 -or
        [BitConverter]::ToUInt32($bytes, $offset) -ne 0x00004550) {
        throw 'Dist executable lacks a valid PE signature/header.'
    }
    if ([BitConverter]::ToUInt16($bytes, $offset + 4) -ne 0x8664) {
        throw 'Dist executable is not x64 (AMD64).'
    }
    $characteristics = [BitConverter]::ToUInt16($bytes, $offset + 22)
    if (($characteristics -band 0x0002) -eq 0 -or
        ($characteristics -band 0x0100) -ne 0 -or ($characteristics -band 0x2000) -ne 0) {
        throw 'Dist PE must be a 64-bit executable, not a DLL.'
    }
    $sections = [BitConverter]::ToUInt16($bytes, $offset + 6)
    $optionalSize = [BitConverter]::ToUInt16($bytes, $offset + 20)
    $optional = $offset + 24
    if ($sections -lt 1 -or $sections -gt 96 -or $optionalSize -lt 112 -or
        $optional + $optionalSize + $sections * 40 -gt $bytes.Length -or
        [BitConverter]::ToUInt16($bytes, $optional) -ne 0x020B) {
        throw 'Dist executable lacks valid PE32+ headers.'
    }
    if ([BitConverter]::ToUInt16($bytes, $optional + 68) -ne 2) {
        throw 'Dist executable must use the Windows GUI subsystem.'
    }
    Write-Host 'Dist PE passed: MZ, PE, x64 executable (not DLL), Windows GUI subsystem.'
}

try {
    if (Test-Path Env:RUSTFLAGS) { throw 'RUSTFLAGS is set. Remove it: it replaces the configured +crt-static rustflags.' }
    $check = Join-Path $PSScriptRoot 'check.ps1'
    & $check
    if ($LASTEXITCODE -ne 0) { throw "check.ps1 failed (exit $LASTEXITCODE)." }

    # check.ps1 exits explicitly; dot-source only its shared helper definitions.
    $checkAst = [System.Management.Automation.Language.Parser]::ParseFile($check, [ref]$null, [ref]$null)
    $helpers = @('Invoke-Checked', 'Get-BuildTools', 'Assert-Imports', 'Assert-Manifest')
    foreach ($helper in $checkAst.EndBlock.Statements) {
        if ($helper -is [System.Management.Automation.Language.FunctionDefinitionAst] -and $helper.Name -in $helpers) {
            . ([scriptblock]::Create($helper.Extent.Text))
        }
    }

    Invoke-Checked cargo @('build', '--profile', 'dist', '--locked')
    $metadata = (Invoke-Checked cargo @('metadata', '--locked', '--no-deps', '--format-version', '1')) | ConvertFrom-Json
    $target = $metadata.target_directory
    if ($env:CARGO_BUILD_TARGET) { $target = Join-Path $target $env:CARGO_BUILD_TARGET }
    $binary = Join-Path $target 'dist/HWIDChecker.exe'
    if (-not (Test-Path -LiteralPath $binary)) { throw "Dist executable not found: $binary" }

    Assert-DistPe $binary
    $tools = Get-BuildTools
    Assert-Manifest $tools.Mt $binary
    Assert-Imports $tools.Dumpbin $binary

    $file = Get-Item -LiteralPath $binary
    $sha256 = (Get-FileHash -LiteralPath $binary -Algorithm SHA256).Hash.ToLowerInvariant()
    Write-Host "Dist executable: $($file.FullName)"
    Write-Host ('Size: {0} bytes ({1:F2} MB)' -f $file.Length, ($file.Length / 1000000))
    Write-Host "SHA-256: $sha256"
    Write-Host "File version: $($file.VersionInfo.FileVersion)"
    exit 0
} catch {
    Write-Error $_.Exception.Message -ErrorAction Continue
    exit 1
}
