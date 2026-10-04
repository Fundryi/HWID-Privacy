#requires -Version 7.0
[CmdletBinding()]
param(
    [switch]$Minor,
    [switch]$Major,
    [string]$Version,
    [switch]$DryRun
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
$PSNativeCommandUseErrorActionPreference = $false
$releaseFiles = @('app/rust/Cargo.toml', 'app/rust/Cargo.lock', 'HWIDChecker.exe')
$originals = @{}
$bumped = $false
$committed = $false
$notesFile = $null
$undo = 'git restore --staged --worktree -- app/rust/Cargo.toml app/rust/Cargo.lock HWIDChecker.exe'

function Invoke-Checked([string]$Program, [string[]]$Arguments, [string]$Failure) {
    & $Program @Arguments
    if ($LASTEXITCODE -ne 0) { throw "$Failure (exit $LASTEXITCODE)." }
}

function Read-Version([string]$Value) {
    if ($Value -cnotmatch '^(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)$') {
        throw 'Version must be X.Y.Z, with three non-negative integers and no leading zeros.'
    }
    $parsed = [version]$Value
    if ($parsed.Major -gt 65535 -or $parsed.Minor -gt 65535 -or $parsed.Build -gt 65535) {
        throw 'Each version component must be at most 65535 for Windows file properties.'
    }
    return $parsed
}

Push-Location $PSScriptRoot
try {
    if ($Minor -and $Major) { throw 'Use either -Minor or -Major, not both.' }
    if ($PSBoundParameters.ContainsKey('Version') -and ($Minor -or $Major)) {
        throw 'Use -Version on its own, without -Minor or -Major.'
    }
    $manifest = Join-Path $PSScriptRoot $releaseFiles[0]
    $cargo = [IO.File]::ReadAllText($manifest)
    $versionPattern = [regex]::new('(?ms)(^\[package\]\s*\r?\n(?:(?!^\[).)*?^version\s*=\s*")([^"]+)(")')
    $match = $versionPattern.Match($cargo)
    if (-not $match.Success) { throw 'Cannot find the package version in app/rust/Cargo.toml.' }
    $current = Read-Version $match.Groups[2].Value
    if ($PSBoundParameters.ContainsKey('Version')) { $next = Read-Version $Version }
    elseif ($Major) { $next = Read-Version "$($current.Major + 1).0.0" }
    elseif ($Minor) { $next = Read-Version "$($current.Major).$($current.Minor + 1).0" }
    else { $next = Read-Version "$($current.Major).$($current.Minor).$($current.Build + 1)" }
    Write-Host "Current $current -> new $next"
    if ($next -le $current) { throw 'The new version must be higher than the current Cargo version.' }
    $tag = "v$next"

    $branch = Invoke-Checked git @('branch', '--show-current') 'Cannot read the current branch'
    if ($branch -cne 'main') { throw 'Release requires the main branch.' }
    $status = @(Invoke-Checked git @('status', '--porcelain') 'Cannot check the working tree')
    if ($status.Count) { throw 'Release requires a clean working tree; commit or stash your changes first.' }
    Invoke-Checked git @('fetch', 'origin', 'main', '--tags') 'Cannot fetch origin/main and tags'
    $behind = Invoke-Checked git @('rev-list', '--count', 'HEAD..origin/main') 'Cannot compare with origin/main'
    if ([int]$behind -ne 0) { throw 'origin/main has commits missing locally; update main first.' }
    Invoke-Checked gh @('auth', 'status') 'GitHub authentication failed; run gh auth login'
    $existingTag = @(Invoke-Checked git @('tag', '--list', $tag) 'Cannot check existing tags')
    if ($existingTag.Count) { throw "Tag $tag already exists." }
    if (Test-Path Env:RUSTFLAGS) { throw 'Remove RUSTFLAGS before releasing; it overrides the static CRT settings.' }
    if ($env:CARGO_BUILD_TARGET -or $env:CARGO_TARGET_DIR -or $env:CARGO_BUILD_TARGET_DIR) {
        throw 'Remove Cargo target overrides before releasing; Publish uses app/rust/target/dist.'
    }
    $head = Invoke-Checked git @('rev-parse', 'HEAD') 'Cannot read HEAD'
    $tags = @(Invoke-Checked git @('tag', '--merged', 'HEAD') 'Cannot list release tags')
    $range = 'HEAD'
    if ($tags.Count) {
        $lastTag = Invoke-Checked git @('describe', '--tags', '--abbrev=0', 'HEAD') 'Cannot find the previous tag'
        $range = "$lastTag..HEAD"
    }
    $subjects = @(Invoke-Checked git @('log', $range, '--format=%s', '--reverse') 'Cannot read release notes')
    $notes = ($subjects | ForEach-Object { "- $_" }) -join "`n"
    if (-not $notes) { $notes = "- Release $tag" }

    if ($DryRun) {
        foreach ($file in $releaseFiles) {
            $originals[$file] = [IO.File]::ReadAllBytes((Join-Path $PSScriptRoot $file))
        }
    }
    $bumped = $true
    $updated = $versionPattern.Replace($cargo, { param($m) $m.Groups[1].Value + $next + $m.Groups[3].Value }, 1)
    [IO.File]::WriteAllText($manifest, $updated, [Text.UTF8Encoding]::new($false))
    Push-Location (Join-Path $PSScriptRoot 'app/rust')
    try {
        # Update the workspace package while retaining locked dependency versions.
        Invoke-Checked cargo @('update', '--workspace') 'Cannot refresh Cargo.lock'
        $metadata = (Invoke-Checked cargo @('metadata', '--locked', '--no-deps', '--format-version', '1') 'Cannot verify Cargo.lock') | ConvertFrom-Json
        if ($metadata.target_directory -ne (Join-Path $PSScriptRoot 'app/rust/target')) {
            throw 'Cargo must use app/rust/target because Publish copies from that directory.'
        }
    } finally { Pop-Location }
    Invoke-Checked pwsh @('-NoProfile', '-File', 'app/rust/check.ps1') 'Build checks failed'
    Invoke-Checked dotnet @('msbuild', 'app/rust/HWIDChecker.Rust.proj', '-t:Publish') 'Publish build failed'

    $binary = Join-Path $PSScriptRoot 'HWIDChecker.exe'
    $info = (Get-Item -LiteralPath $binary).VersionInfo
    if ($info.FileVersion -cne "$next" -or $info.ProductVersion -cne "$next") {
        throw "Published executable version does not match Cargo $next."
    }
    $hash = (Get-FileHash -LiteralPath $binary -Algorithm SHA256).Hash.ToLowerInvariant()
    Write-Host "File version: $($info.FileVersion)"
    Write-Host "SHA-256: $hash"
    Write-Host 'Commits that will go live (plus the release commit):'
    Invoke-Checked git @('log', 'origin/main..HEAD', '--oneline') 'Cannot list commits to publish'
    if ($DryRun) {
        Write-Host 'Dry run passed. No commit, tag, push or GitHub release was made.'
        return
    }

    $notesFile = [IO.Path]::GetTempFileName()
    [IO.File]::WriteAllText($notesFile, $notes, [Text.UTF8Encoding]::new($false))
    $answer = Read-Host "Current $current -> new $next. Publish $tag to all users? Type YES"
    if ($answer -cne 'YES') {
        Write-Host "Release cancelled. Undo the local version bump and staged exe with: $undo"
        return
    }
    $nowBranch = Invoke-Checked git @('branch', '--show-current') 'Cannot recheck the branch'
    $nowHead = Invoke-Checked git @('rev-parse', 'HEAD') 'Cannot recheck HEAD'
    if ($nowBranch -cne 'main' -or $nowHead -cne $head) { throw 'Branch or HEAD changed during the build; start again.' }
    $changed = @(Invoke-Checked git @('diff', 'HEAD', '--name-only') 'Cannot recheck changed files')
    $untracked = @(Invoke-Checked git @('ls-files', '--others', '--exclude-standard') 'Cannot recheck untracked files')
    if (@($changed | Where-Object { $_ -notin $releaseFiles }).Count -or $untracked.Count) {
        throw 'Other files changed during the build; review them before releasing.'
    }
    if ((Get-FileHash -LiteralPath $binary -Algorithm SHA256).Hash.ToLowerInvariant() -cne $hash) {
        throw 'The root executable changed after verification; start again.'
    }
    Invoke-Checked git (@('add', '--') + $releaseFiles) 'Cannot stage the release files'
    Invoke-Checked git (@('commit', '-m', "Release $tag", '--only', '--') + $releaseFiles) 'Cannot commit the release'
    $committed = $true
    Invoke-Checked git @('tag', $tag) 'Cannot tag the release'
    Invoke-Checked git @('push', '--atomic', 'origin', 'main', "refs/tags/$tag") 'Cannot push main and the release tag'
    Invoke-Checked gh @('release', 'create', $tag, 'HWIDChecker.exe', '--verify-tag', '--title', "HWIDChecker $tag", '--notes-file', $notesFile) 'Cannot create the GitHub release; main and its tag have already been pushed'
    Write-Host 'Installed copies get this update on their next update check.'
} catch {
    [Console]::Error.WriteLine("Error: $($_.Exception.Message)")
    if ($bumped -and -not $DryRun) {
        if ($committed) { Write-Host 'The release commit exists. Review git status, the tag and origin before retrying.' }
        else { Write-Host "Undo the local version bump and staged exe with: $undo" }
    }
    exit 1
} finally {
    if ($DryRun -and $bumped) {
        foreach ($file in $releaseFiles) {
            [IO.File]::WriteAllBytes((Join-Path $PSScriptRoot $file), $originals[$file])
        }
        Write-Host 'Restored Cargo.toml, Cargo.lock and the root HWIDChecker.exe.'
    }
    if ($notesFile) { Remove-Item -LiteralPath $notesFile }
    Pop-Location
}
