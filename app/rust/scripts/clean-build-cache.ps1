#requires -Version 7.0
[CmdletBinding()]
param([switch]$Apply)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
$PSNativeCommandUseErrorActionPreference = $false
$RustRoot = [IO.Path]::GetFullPath((Split-Path -Parent $PSScriptRoot))
$debug = Join-Path $RustRoot 'target/debug'
$incremental = Join-Path $debug 'incremental'
$handles = [Collections.Generic.List[IO.FileStream]]::new()
$count = 0
$bytes = 0L

function Assert-PlainPath([string]$Path) {
    $item = Get-Item -LiteralPath $Path -Force
    if ($item.Attributes -band [IO.FileAttributes]::ReparsePoint) {
        throw "Refusing a reparse point: $Path"
    }
    return $item
}

function Get-SessionFiles([string]$Path) {
    $null = Assert-PlainPath $Path
    foreach ($file in Get-ChildItem -LiteralPath $Path -Force) {
        if ($file.PSIsContainer -or ($file.Attributes -band [IO.FileAttributes]::ReparsePoint) -or
            $file.Name -cnotmatch '^(dep-graph(\.part)?\.bin|query-cache\.bin|work-products\.bin|metadata\.rmeta|[a-z0-9]+\.o)$') {
            throw "Unrecognized incremental content; preserving $Path"
        }
        $file
    }
}

Push-Location $RustRoot
try {
    # This parser follows the pinned compiler's internal layout. Re-review on a toolchain bump.
    # https://github.com/rust-lang/rust/blob/1.98.0/compiler/rustc_incremental/src/persist/fs.rs
    if ((Get-Content -LiteralPath 'rust-toolchain.toml' -Raw) -notmatch 'channel\s*=\s*"1\.98\.0"') {
        throw 'Cleanup supports the pinned Rust 1.98.0 layout only.'
    }
    $compiler = & rustc --version
    if ($LASTEXITCODE -ne 0 -or $compiler -cnotmatch '^rustc 1\.98\.0 ') {
        throw 'Cleanup requires the pinned Rust 1.98.0 compiler.'
    }
    foreach ($name in @('CARGO_TARGET_DIR', 'CARGO_BUILD_TARGET', 'CARGO_BUILD_TARGET_DIR',
        'CARGO_BUILD_BUILD_DIR', 'CARGO_BUILD_DIR')) {
        if ([Environment]::GetEnvironmentVariable($name)) { throw "Cleanup refuses $name overrides." }
    }
    if (-not (Test-Path -LiteralPath $incremental)) {
        Write-Host 'Build cache cleanup: no incremental sessions.'
        return
    }
    # Check every ancestor before entering the target tree; never follow a junction.
    $ancestor = $RustRoot
    while ($ancestor) {
        $null = Assert-PlainPath $ancestor
        $ancestor = Split-Path -Parent $ancestor
    }
    foreach ($path in @((Join-Path $RustRoot 'target'), $debug, $incremental)) {
        $null = Assert-PlainPath $path
    }
    $tracked = @(& git ls-files -- target/debug/incremental)
    if ($LASTEXITCODE -ne 0 -or $tracked.Count) { throw 'Cleanup requires an untracked incremental cache.' }
    # Exclusive Windows opens block Cargo, including direct invocations, for this profile.
    foreach ($name in @('.cargo-lock', '.cargo-build-lock', '.cargo-artifact-lock')) {
        $path = Join-Path $debug $name
        $null = Assert-PlainPath $path
        $handles.Add([IO.File]::Open($path, [IO.FileMode]::Open, [IO.FileAccess]::ReadWrite, [IO.FileShare]::None))
    }
    $cutoff = [DateTimeOffset]::UtcNow.ToUnixTimeMilliseconds() * 1000L - 30000000L
    foreach ($crate in Get-ChildItem -LiteralPath $incremental -Directory -Force) {
        $crateHandles = [Collections.Generic.List[IO.FileStream]]::new()
        try {
            $null = Assert-PlainPath $crate.FullName
            if ($crate.Name -cnotmatch '^[A-Za-z0-9_]+-[a-z0-9]{13}$') { continue }
            $sessions = @()
            foreach ($dir in Get-ChildItem -LiteralPath $crate.FullName -Directory -Force) {
                $null = Assert-PlainPath $dir.FullName
                if ($dir.Name -cnotmatch '^s-([a-z0-9]{10})-([a-z0-9]{7})-([a-z0-9]{25}|working)$') {
                    throw "Unrecognized session; preserving $($crate.FullName)"
                }
                $stamp = $Matches[1]
                $lock = Join-Path $crate.FullName ("s-$stamp-$($Matches[2]).lock")
                $working = $Matches[3] -ceq 'working'
                $micros = 0L
                foreach ($char in $stamp.ToCharArray()) {
                    $micros = $micros * 36L + '0123456789abcdefghijklmnopqrstuvwxyz'.IndexOf($char)
                }
                $null = Assert-PlainPath $lock
                $crateHandles.Add([IO.File]::Open($lock, [IO.FileMode]::Open, [IO.FileAccess]::ReadWrite, [IO.FileShare]::None))
                $files = @(Get-SessionFiles $dir.FullName)
                if (-not $working -and 'dep-graph.bin' -cnotin $files.Name) {
                    throw "Incomplete finalized session; preserving $($crate.FullName)"
                }
                $sessions += [pscustomobject]@{ Directory = $dir; Files = $files; Stamp = $stamp; Micros = $micros; Working = $working }
            }
            # Fixed-width base-36 timestamps sort chronologically. Keep ALL ties at the newest stamp.
            $finalized = @($sessions | Where-Object { -not $_.Working } | Sort-Object Stamp -Descending)
            $newest = if ($finalized.Count) { $finalized[0].Stamp } else { $null }
            foreach ($session in $sessions) {
                if ($session.Micros -ge $cutoff -or
                    (-not $session.Working -and [string]::CompareOrdinal($session.Stamp, $newest) -ge 0)) { continue }
                $path = [IO.Path]::GetFullPath($session.Directory.FullName)
                if ([IO.Path]::GetDirectoryName($path) -cne $crate.FullName -or
                    -not $path.StartsWith("$incremental$([IO.Path]::DirectorySeparatorChar)", [StringComparison]::OrdinalIgnoreCase)) {
                    throw "Refusing a path outside the incremental cache: $path"
                }
                $total = ($session.Files | Measure-Object -Property Length -Sum).Sum
                if ($Apply) {
                    $null = Assert-PlainPath $incremental
                    $null = Assert-PlainPath $crate.FullName
                    $null = Assert-PlainPath $path
                    # Flat files only: never recursively delete unknown data or change hard-link attributes.
                    foreach ($file in $session.Files) { Remove-Item -LiteralPath $file.FullName }
                    [IO.Directory]::Delete($path, $false)
                } else {
                    Write-Host ("Would remove: {0} ({1:N0} bytes)" -f $path, $total)
                }
                $count++
                $bytes += [long]$total
            }
        } catch {
            Write-Warning "Build cache cleanup skipped a crate: $($_.Exception.Message)"
        } finally {
            foreach ($handle in $crateHandles) { $handle.Dispose() }
        }
    }
    $action = if ($Apply) { 'removed' } else { 'would remove' }
    Write-Host ("Build cache cleanup: {0} {1} obsolete sessions ({2:F2} MB logical; hard links may share disk space). Latest caches kept." -f $action, $count, ($bytes / 1000000))
} catch {
    Write-Error "Build cache cleanup refused: $($_.Exception.Message)" -ErrorAction Continue
    exit 1
} finally {
    foreach ($handle in $handles) { $handle.Dispose() }
    Pop-Location
}
