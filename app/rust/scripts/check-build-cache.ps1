#requires -Version 7.0
# Deletion-boundary checks in an isolated checkout; no real build caches are touched.
[CmdletBinding()]
param()

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
$PSNativeCommandUseErrorActionPreference = $false
$sandbox = Join-Path ([IO.Path]::GetTempPath()) ("HWID-cache-check-" + [guid]::NewGuid())
$rust = Join-Path $sandbox 'app/rust'
$debug = Join-Path $rust 'target/debug'
$incremental = Join-Path $debug 'incremental'
$source = Join-Path $PSScriptRoot 'clean-build-cache.ps1'
$cleanup = Join-Path $rust 'scripts/clean-build-cache.ps1'
$held = $null

function Assert([bool]$Condition, [string]$Message) {
    if (-not $Condition) { throw $Message }
}

function New-Session([string]$Crate, [string]$Stamp, [string]$Suffix = 'aaaaaaaaaaaaaaaaaaaaaaaaa') {
    $parent = Join-Path $incremental "$Crate-0000000000000"
    $path = Join-Path $parent "s-$Stamp-0000000-$Suffix"
    $null = New-Item -ItemType Directory -Path $path -Force
    [IO.File]::WriteAllText((Join-Path $path 'dep-graph.bin'), 'cache')
    [IO.File]::WriteAllText((Join-Path $parent "s-$Stamp-0000000.lock"), '')
    return $path
}

function Run-Cleanup([string[]]$Arguments = @()) {
    $output = @(& pwsh -NoProfile -File $cleanup @Arguments 2>&1)
    return @{ Code = $LASTEXITCODE; Output = ($output -join "`n") }
}

try {
    $null = New-Item -ItemType Directory -Path (Split-Path $cleanup), $debug -Force
    Copy-Item -LiteralPath $source -Destination $cleanup
    [IO.File]::WriteAllText((Join-Path $rust 'rust-toolchain.toml'), "[toolchain]`nchannel = `"1.98.0`"`n")
    $null = & git init --quiet $sandbox
    Assert ($LASTEXITCODE -eq 0) 'Cannot initialize the disposable checkout.'
    foreach ($name in @('.cargo-lock', '.cargo-build-lock', '.cargo-artifact-lock')) {
        [IO.File]::WriteAllText((Join-Path $debug $name), '')
    }
    # Directory modification times are intentionally reversed; compiler-name timestamps win.
    $old = New-Session 'crate' '0000000001'
    $latest = New-Session 'crate' '0000000002'
    (Get-Item -LiteralPath $old).LastWriteTimeUtc = [datetime]::UtcNow
    (Get-Item -LiteralPath $latest).LastWriteTimeUtc = [datetime]::UtcNow.AddDays(-1)
    $working = New-Session 'crate' '0000000003' 'working'
    $sole = New-Session 'other_configuration' '0000000001'
    $tie1 = New-Session 'ties' '0000000002'
    $tie2 = New-Session 'ties' '0000000002' 'bbbbbbbbbbbbbbbbbbbbbbbbb'
    $unknown = New-Session 'unknown' '0000000001'
    $null = New-Session 'unknown' '0000000002'
    [IO.File]::WriteAllText((Join-Path $unknown 'report.txt'), 'preserve')
    $locked = New-Session 'locked' '0000000001'
    $null = New-Session 'locked' '0000000002'
    $lock = Join-Path (Split-Path $locked) 's-0000000001-0000000.lock'
    $held = [IO.File]::Open($lock, [IO.FileMode]::Open, [IO.FileAccess]::ReadWrite, [IO.FileShare]::ReadWrite)
    $held.Lock(0, 1)
    $settings = Join-Path $debug 'HWIDChecker.settings.json'
    [IO.File]::WriteAllText($settings, 'preserve')
    $outside = Join-Path $sandbox 'protected'
    $null = New-Item -ItemType Directory -Path $outside
    [IO.File]::WriteAllText((Join-Path $outside 'dep-graph.bin'), 'outside')
    $junction = Join-Path $incremental 'linked-0000000000000'
    $null = New-Item -ItemType Junction -Path $junction -Target $outside

    $preview = Run-Cleanup
    Assert ($preview.Code -eq 0 -and $preview.Output.Contains('would remove 2 obsolete sessions')) 'Preview did not select exactly the old and working sessions.'
    Assert ((Test-Path $old) -and (Test-Path $working)) 'Preview deleted files.'
    $applied = Run-Cleanup @('-Apply')
    Assert ($applied.Code -eq 0 -and $applied.Output.Contains('removed 2 obsolete sessions')) 'Apply failed.'
    Assert (-not (Test-Path $old) -and -not (Test-Path $working)) 'Obsolete sessions remain.'
    foreach ($path in @($latest, $sole, $tie1, $tie2, $unknown, $locked, $settings, (Join-Path $outside 'dep-graph.bin'))) {
        Assert (Test-Path -LiteralPath $path) "Protected data was deleted: $path"
    }
    $held.Dispose()
    $held = $null
    $held = [IO.File]::Open((Join-Path $debug '.cargo-build-lock'), [IO.FileMode]::Open, [IO.FileAccess]::ReadWrite, [IO.FileShare]::ReadWrite)
    $held.Lock(0, 1)
    $busy = Run-Cleanup @('-Apply')
    Assert ($busy.Code -ne 0 -and (Test-Path $locked)) 'A busy Cargo profile was not refused.'
    $held.Dispose()
    $held = $null
    # A linked profile must be rejected before acquiring locks or deleting anything.
    Remove-Item -LiteralPath $junction
    $linkedTarget = Join-Path $rust 'target/debug-link'
    $null = New-Item -ItemType Junction -Path $linkedTarget -Target $debug
    $realIncremental = Join-Path $debug 'incremental-real'
    Move-Item -LiteralPath $incremental -Destination $realIncremental
    $null = New-Item -ItemType Junction -Path $incremental -Target $realIncremental
    $linked = Run-Cleanup @('-Apply')
    Assert ($linked.Code -ne 0) 'An incremental junction was not refused.'
    Remove-Item -LiteralPath $incremental, $linkedTarget
    Move-Item -LiteralPath $realIncremental -Destination $incremental
    Push-Location $rust
    try {
        $null = & git add -f -- ('target/debug/incremental/locked-0000000000000/' + (Split-Path $locked -Leaf) + '/dep-graph.bin')
        Assert ($LASTEXITCODE -eq 0) 'Cannot stage the tracked-cache guard input.'
    } finally { Pop-Location }
    $tracked = Run-Cleanup @('-Apply')
    Assert ($tracked.Code -ne 0 -and (Test-Path $locked)) 'Tracked cache files were not protected.'
    Write-Host 'Build cache safety checks passed: preview, latest/ties/configurations, abandoned work, unknown files, compiler/Cargo locks, junctions, tracked files, and settings.'
} finally {
    if ($held) { $held.Dispose() }
    # Remove junctions as links before deleting the explicitly named disposable root.
    foreach ($link in @((Join-Path $rust 'target/debug-link'), (Join-Path $debug 'incremental'), (Join-Path $incremental 'linked-0000000000000'))) {
        if ((Test-Path -LiteralPath $link) -and ((Get-Item -LiteralPath $link -Force).Attributes -band [IO.FileAttributes]::ReparsePoint)) {
            Remove-Item -LiteralPath $link
        }
    }
    if ([IO.Path]::GetFullPath($sandbox).StartsWith([IO.Path]::GetFullPath([IO.Path]::GetTempPath()), [StringComparison]::OrdinalIgnoreCase) -and
        (Split-Path $sandbox -Leaf) -match '^HWID-cache-check-[a-f0-9-]{36}$') {
        Remove-Item -LiteralPath $sandbox -Recurse -Force
    }
}
