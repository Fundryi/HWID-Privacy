#requires -Version 7.0
[CmdletBinding()]
param(
    [switch]$Safe,
    [switch]$Run
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
$PSNativeCommandUseErrorActionPreference = $false
$override = $env:HWID_ALLOW_DESTRUCTIVE
$RustRoot = Split-Path -Parent $PSScriptRoot
Push-Location $RustRoot
try {
    $arguments = @('build', '--locked')
    $profile = 'debug'
    if (-not $Safe) {
        $arguments += '--release'
        $profile = 'release'
    }
    & cargo @arguments
    if ($LASTEXITCODE -ne 0) { throw "Cargo build failed (exit $LASTEXITCODE)." }
    $json = & cargo metadata --locked --no-deps --format-version 1
    if ($LASTEXITCODE -ne 0) { throw "Cargo metadata failed (exit $LASTEXITCODE)." }
    $target = ($json | ConvertFrom-Json).target_directory
    if ($env:CARGO_BUILD_TARGET) { $target = Join-Path $target $env:CARGO_BUILD_TARGET }
    $binary = Join-Path $target "$profile/HWIDChecker.exe"
    if (-not (Test-Path -LiteralPath $binary)) { throw "Test executable not found: $binary" }
    Write-Host "Test executable: $binary"
    if ($Safe) {
        Write-Host 'Safe debug build: destructive actions are dry runs when launched with -Safe -Run.'
    }
    if ($Run) {
        # Only the launched child loses the override; restore the caller's environment below.
        if ($Safe) { Remove-Item Env:HWID_ALLOW_DESTRUCTIVE -ErrorAction SilentlyContinue }
        Start-Process -FilePath $binary -WorkingDirectory (Split-Path $binary) | Out-Null
    }
} catch {
    [Console]::Error.WriteLine("Error: $($_.Exception.Message)")
    exit 1
} finally {
    $env:HWID_ALLOW_DESTRUCTIVE = $override
    Pop-Location
}
