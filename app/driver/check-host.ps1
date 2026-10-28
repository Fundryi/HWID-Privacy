#requires -Version 7.0
[CmdletBinding()]
param()
Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
$PSNativeCommandUseErrorActionPreference = $false
. (Join-Path $PSScriptRoot 'scripts/tools.ps1')
Push-Location $PSScriptRoot
try {
    $visualStudio = Get-VisualStudio
    Enter-DriverHostEnvironment $visualStudio
    $hostOut = Join-Path $PSScriptRoot 'out/host'
    New-Item -ItemType Directory -Path $hostOut -Force | Out-Null
    # Compile the ACTUAL portable driver logic, not a reimplementation of it.
    Invoke-Checked 'cl.exe' @('/nologo', '/std:c17', '/W4', '/WX', '/sdl', '/GS', '/guard:cf', '/MT', '/Od', '/utf-8',
        '/Iinclude', '/Isrc', 'tests/host.c', 'src/protocol.c', 'src/smbus.c',
        "/Fo$hostOut/", "/Fe$hostOut/host.exe", '/link', '/NXCOMPAT', '/DYNAMICBASE')
    Invoke-Checked (Join-Path $hostOut 'host.exe') @()
    Invoke-Checked 'cargo' @('fmt', '--check')
    Invoke-Checked 'cargo' @('clippy', '--all-targets', '--', '-D', 'warnings')
    Invoke-Checked 'cargo' @('test')
    [xml]$project = Get-Content -LiteralPath 'HWIDReader.vcxproj' -Raw
    foreach ($script in @('build.ps1', 'check-host.ps1', 'scripts/tools.ps1')) {
        $errors = $null
        [void][System.Management.Automation.Language.Parser]::ParseFile((Join-Path $PSScriptRoot $script), [ref]$null, [ref]$errors)
        if ($errors.Count -ne 0) { throw "PowerShell syntax errors in ${script}: $errors" }
    }
    Write-Host 'Driver host checks passed. WDK compilation, InfVerif and hardware checks are still pending.'
} catch {
    Write-Error $_.Exception.Message -ErrorAction Continue
    exit 1
} finally { Pop-Location }
