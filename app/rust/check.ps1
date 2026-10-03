#requires -Version 7.0
[CmdletBinding()]
param(
    [switch]$Golden,
    [string[]]$Sections = @(),
    [switch]$Ghosts,
    [switch]$Logs,
    [switch]$Timing
)

Set-Location $PSScriptRoot
Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
$PSNativeCommandUseErrorActionPreference = $false
$SectionTitles = @(
    'DISK DRIVES', 'MOTHERBOARD', 'CHASSIS', '(SM)BIOS',
    'SYSTEM INFORMATION', 'RAM MODULES', 'CPU', 'TPM MODULES',
    'USB DEVICES', 'GPU INFO', 'MONITOR INFORMATION',
    "NETWORK ADAPTERS (NIC's)", 'BLUETOOTH ADAPTERS', 'ARP INFO/CACHE'
)
$Utf8 = [System.Text.UTF8Encoding]::new($false, $true)

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

# Both tools write identifiers directly to the private capture folder, never to a temp folder.
function Invoke-Capture([string]$Exe, [string[]]$Arguments, [string]$Output, [int]$TimeoutSeconds = 180) {
    if (Test-Path -LiteralPath $Output) { throw "Refusing to reuse a capture: $Output" }
    $start = [System.Diagnostics.ProcessStartInfo]::new($Exe)
    $start.UseShellExecute = $false
    $start.CreateNoWindow = $true
    foreach ($argument in $Arguments) { $start.ArgumentList.Add($argument) }
    $process = [System.Diagnostics.Process]::new()
    $process.StartInfo = $start
    try {
        if (-not $process.Start()) { throw "Could not start $Exe" }
        if (-not $process.WaitForExit($TimeoutSeconds * 1000)) {
            $process.Kill($true)
            if (-not $process.WaitForExit(5000)) { throw "$Exe timed out; process termination also timed out." }
            throw "$Exe timed out after $TimeoutSeconds seconds."
        }
        if ($process.ExitCode -ne 0) { throw "$Exe failed (exit $($process.ExitCode)). Capture: $Output" }
        if (-not (Test-Path -LiteralPath $Output)) { throw "$Exe did not write $Output" }
    } finally { $process.Dispose() }
}

function Read-Utf8([string]$Path) {
    return $Utf8.GetString([System.IO.File]::ReadAllBytes($Path))
}

function Assert-Implemented([string]$Text, [string]$Label) {
    if ($Text -match '(?i)\bnot[ -]ported\b|\bunimplemented\b|\bstub\b|\bTODO\b') {
        throw "$Label is still a stub."
    }
}

# String equality is byte equality for strict UTF-8, with the BOM retained as U+FEFF.
# Split only on LF so JSON displays CR, padding, BOM and missing final newlines.
function Compare-Lines([string]$Before, [string]$After, [string]$Section,
    [string]$LeftLabel = 'C#', [string]$RightLabel = 'Rust',
    [System.Collections.Generic.Dictionary[string,string]]$ApprovedLines = $null) {
    if ([string]::Equals($Before, $After, [System.StringComparison]::Ordinal)) { return $true }
    $left = $Before.Split("`n")
    $right = $After.Split("`n")
    $ok = $true
    for ($i = 0; $i -lt [Math]::Max($left.Count, $right.Count); $i++) {
        $a = if ($i -lt $left.Count) { ConvertTo-Json -InputObject $left[$i] -Compress -EscapeHandling EscapeNonAscii } else { '<missing>' }
        $b = if ($i -lt $right.Count) { ConvertTo-Json -InputObject $right[$i] -Compress -EscapeHandling EscapeNonAscii } else { '<missing>' }
        if ($i -ge $left.Count -or $i -ge $right.Count -or
            -not [string]::Equals($left[$i], $right[$i], [System.StringComparison]::Ordinal)) {
            $approval = $null
            if ($null -ne $ApprovedLines -and $i -lt $right.Count) {
                # The allowlist excludes line terminators, but preserves all other characters.
                $line = $right[$i]
                if ($line.EndsWith("`r", [System.StringComparison]::Ordinal)) { $line = $line.Substring(0, $line.Length - 1) }
                $key = $Section + [char]0 + $line
                if ($ApprovedLines.ContainsKey($key)) { $approval = $ApprovedLines[$key] }
            }
            $status = if ($null -ne $approval) { "approved ($approval)" } else { 'needs approval' }
            Write-Host "[$Section] line $($i + 1) ${status}: $LeftLabel=$a $RightLabel=$b"
            if ($null -eq $approval) { $ok = $false }
        }
    }
    return $ok
}

function Read-ApprovedLines([string]$Path) {
    $lines = [System.Collections.Generic.Dictionary[string,string]]::new([System.StringComparer]::Ordinal)
    if (-not (Test-Path -LiteralPath $Path)) { return ,$lines }
    foreach ($entry in [System.IO.File]::ReadAllLines($Path, $Utf8)) {
        $columns = $entry.Split("`t", 3, [System.StringSplitOptions]::None)
        if ($columns.Count -ne 3 -or $columns[0] -cnotmatch '^AD-[0-9]{2}$' -or
            ($columns[1] -cne 'REPORT HEADER' -and $columns[1] -cnotin $SectionTitles)) {
            throw "Invalid reviewed-line entry in ${Path}: expected AD-xx<TAB>section<TAB>exact Rust line."
        }
        $lines[$columns[1] + [char]0 + $columns[2]] = $columns[0]
    }
    return ,$lines
}

function Read-Report([string]$Path, [string[]]$ExpectedTitles) {
    $text = Read-Utf8 $Path
    # C# parity: app/src/Services/TextFormattingService.cs:7,20-41 (93-column headers).
    $headers = [regex]::Matches($text, '(?m)^={93}\r?\n *(?<title>[^\r\n]+)\r?\n={93}\r?\n')
    # A BOM belongs to the preamble and must participate in comparison.
    if ($text.StartsWith([string][char]0xFEFF, [System.StringComparison]::Ordinal)) {
        $headers = [regex]::Matches($text.Substring(1), '(?m)^={93}\r?\n *(?<title>[^\r\n]+)\r?\n={93}\r?\n')
        $offset = 1
    } else { $offset = 0 }
    if ($headers.Count -ne $ExpectedTitles.Count + 1 -or $headers[0].Index -ne 0 -or
        -not [string]::Equals($headers[0].Groups['title'].Value, 'Comprehensive HWID Checker', [System.StringComparison]::Ordinal)) {
        throw "Invalid report header or section count in $Path; expected $($ExpectedTitles.Count) sections."
    }
    $parts = [ordered]@{}
    for ($i = 1; $i -lt $headers.Count; $i++) {
        $title = $headers[$i].Groups['title'].Value
        if (-not [string]::Equals($title, $ExpectedTitles[$i - 1], [System.StringComparison]::Ordinal)) {
            throw "Unexpected section $i in ${Path}: $title"
        }
        $start = $headers[$i].Index + $offset
        $end = if ($i + 1 -lt $headers.Count) { $headers[$i + 1].Index + $offset } else { $text.Length }
        $parts[$title] = $text.Substring($start, $end - $start)
    }
    return @{ Preamble = $text.Substring(0, $headers[1].Index + $offset); Parts = $parts }
}

function Sort-UsbGroups([string]$Section) {
    $header = [regex]::Match($Section, '\A={93}\r?\n[^\r\n]+\r?\n={93}\r?\n')
    if (-not $header.Success) { throw 'USB section has no valid header.' }
    $body = $Section.Substring($header.Length)
    if (-not $body.StartsWith('Device:', [System.StringComparison]::Ordinal)) { return $Section }
    # C# parity: app/src/Services/TextFormattingService.cs:8,44-47,65-79 (40 dashes between groups).
    $separator = ('-' * 40) + "`r`n"
    [string[]]$groups = $body.Split(@($separator), [System.StringSplitOptions]::None)
    [Array]::Sort($groups, [System.StringComparer]::Ordinal)
    return $header.Value + [string]::Join($separator, $groups)
}

function Compare-Ghosts([string]$Before, [string]$After) {
    $legacy = [System.Collections.Generic.Dictionary[string, int]]::new([System.StringComparer]::Ordinal)
    $rust = [System.Collections.Generic.Dictionary[string, int]]::new([System.StringComparer]::Ordinal)
    $absent = [System.Collections.Generic.Dictionary[string, int]]::new([System.StringComparer]::Ordinal)
    foreach ($side in @(@{ Path = $Before; Rust = $false }, @{ Path = $After; Rust = $true })) {
        $text = Read-Utf8 $side.Path
        Assert-Implemented $text $side.Path
        foreach ($line in $text.Split("`r`n", [System.StringSplitOptions]::RemoveEmptyEntries)) {
            $columns = $line.Split(@(' | '), [System.StringSplitOptions]::None)
            if ($columns.Count -ne 5) { throw "Malformed ghost row in $($side.Path)" }
            $key = [string]::Join(' | ', $columns[0..2])
            $set = if ($side.Rust) { $rust } else { $legacy }
            if (-not $set.ContainsKey($key)) { $set[$key] = 0 }
            $set[$key]++
            if ($side.Rust) {
                if ([Array]::IndexOf(@('Absent', 'Unclear'), $columns[4]) -lt 0) { throw "Invalid Rust ghost presence: $($columns[4])" }
                if ([string]::Equals($columns[4], 'Absent', [System.StringComparison]::Ordinal)) {
                    if (-not $absent.ContainsKey($key)) { $absent[$key] = 0 }
                    $absent[$key]++
                }
            }
        }
    }
    $different = $false
    foreach ($key in $legacy.Keys) {
        if (-not $rust.ContainsKey($key) -or $rust[$key] -lt $legacy[$key]) {
            Write-Host "[GHOSTS] needs approval: C# ghost missing in Rust: $key"
            $different = $true
        }
    }
    foreach ($key in $absent.Keys) {
        if (-not $legacy.ContainsKey($key) -or $legacy[$key] -lt $absent[$key]) {
            Write-Host "[GHOSTS] needs approval: Rust Absent device missing in C#: $key"
            $different = $true
        }
    }
    if ($different) { throw 'Ghost membership checks failed.' }
    Write-Host "Ghost membership checks passed ($($legacy.Count) C# and $($rust.Count) Rust device keys)."
}

function Read-Timings([string]$Path) {
    $lines = (Read-Utf8 $Path).Split("`r`n", [System.StringSplitOptions]::RemoveEmptyEntries)
    if ($lines.Count -ne 15 -or $lines[0] -cnotin @("Section`tMedianMs", "Section`tMedian ms")) {
        throw "Invalid timing header or row count in $Path"
    }
    $times = [ordered]@{}
    for ($i = 1; $i -lt $lines.Count; $i++) {
        $row = $lines[$i].Split("`t")
        if ($row.Count -ne 2 -or $row[0] -cne $SectionTitles[$i - 1]) { throw "Invalid timing row $i in $Path" }
        $value = [double]::Parse($row[1], [System.Globalization.CultureInfo]::InvariantCulture)
        if (-not [double]::IsFinite($value) -or $value -lt 0) { throw "Invalid timing value in $Path" }
        $times[$row[0]] = $value
    }
    return $times
}

try {
    if (Test-Path Env:RUSTFLAGS) { throw 'RUSTFLAGS is set. Remove it: it replaces the configured +crt-static rustflags.' }
    if (Test-Path Env:CARGO_ENCODED_RUSTFLAGS) { throw 'CARGO_ENCODED_RUSTFLAGS is set; remove this rustflags override.' }
    $modes = @($Golden, $Ghosts, $Logs, $Timing).Where({ $_ }).Count
    if ($modes -gt 1) { throw 'Choose one mode: -Golden, -Ghosts, -Logs, or -Timing.' }
    if ($Sections.Count -gt 0 -and -not $Golden) { throw '-Sections requires -Golden.' }
    foreach ($title in $Sections) {
        if ([Array]::IndexOf($SectionTitles, $title) -lt 0) { throw "Unknown section title: $title" }
    }
    if (@($Sections | Select-Object -Unique).Count -ne $Sections.Count) { throw 'Duplicate section titles.' }
    if ($modes) {
        $identity = [System.Security.Principal.WindowsIdentity]::GetCurrent()
        try {
            $principal = [System.Security.Principal.WindowsPrincipal]::new($identity)
            if (-not $principal.IsInRole([System.Security.Principal.WindowsBuiltInRole]::Administrator)) {
                throw 'Hardware comparison modes require elevation. Run check.ps1 from an elevated PowerShell (Run as administrator).'
            }
        } finally { $identity.Dispose() }
    }

    Invoke-Checked cargo @('fmt', '--check')
    Invoke-Checked cargo @('clippy', '--locked', '--all-targets', '--', '-D', 'warnings')
    Invoke-Checked cargo @('test', '--locked')
    Invoke-Checked cargo @('build', '--release', '--locked')
    $metadata = (Invoke-Checked cargo @('metadata', '--locked', '--no-deps', '--format-version', '1')) | ConvertFrom-Json
    $target = $metadata.target_directory
    if ($env:CARGO_BUILD_TARGET) { $target = Join-Path $target $env:CARGO_BUILD_TARGET }
    $binary = Join-Path $target 'release/HWIDChecker.exe'
    if (-not (Test-Path -LiteralPath $binary)) { throw "Release executable not found: $binary" }
    $tools = Get-BuildTools
    Assert-Imports $tools.Dumpbin $binary
    Assert-Manifest $tools.Mt $binary
    Write-Host 'All base checks passed.'
    if ($modes -eq 0) { exit 0 }

    $common = (Invoke-Checked git @('rev-parse', '--path-format=absolute', '--git-common-dir')).Trim()
    $mainRoot = Split-Path -Parent $common
    $repoRoot = (Invoke-Checked git @('rev-parse', '--show-toplevel')).Trim()
    $harness = Join-Path $repoRoot 'app/tools/GoldenDump/GoldenDump.csproj'
    if (-not (Test-Path -LiteralPath $harness)) {
        throw 'GoldenDump is missing. Merge app/tools/GoldenDump before running hardware comparisons.'
    }
    $approved = Join-Path $mainRoot 'docs/rust-port/approved-diffs.md'
    if ($Golden -and -not (Test-Path -LiteralPath $approved)) { throw "Approval ledger is missing: $approved" }
    $captureRoot = Join-Path $mainRoot 'app/rust/golden/owner-pc'
    $run = Join-Path $captureRoot ((Get-Date -Format 'yyyyMMdd-HHmmss') + '-' + [guid]::NewGuid().ToString('N'))
    New-Item -ItemType Directory -Path $run | Out-Null
    # Query MSBuild's resolved TargetPath instead of assuming a bin directory layout.
    Invoke-Checked dotnet @('build', $harness, '-c', 'Release')
    $harnessDll = (Invoke-Checked dotnet @('msbuild', $harness, '-nologo', '-p:Configuration=Release', '-getProperty:TargetPath')).Trim()
    $harnessExe = [System.IO.Path]::ChangeExtension($harnessDll, '.exe')
    if (-not (Test-Path -LiteralPath $harnessExe)) { throw "GoldenDump executable not found: $harnessExe" }
    @{
        RustCommit = (Invoke-Checked git @('rev-parse', 'HEAD')).Trim()
        CSharpCommit = (Invoke-Checked git @('-C', $repoRoot, 'rev-parse', 'HEAD')).Trim()
        DirtyFiles = @(Invoke-Checked git @('status', '--porcelain'))
        RustExeSha256 = (Get-FileHash -LiteralPath $binary -Algorithm SHA256).Hash
        GoldenDumpExeSha256 = (Get-FileHash -LiteralPath $harnessExe -Algorithm SHA256).Hash
        CSharpAssemblySha256 = (Get-FileHash -LiteralPath (Join-Path (Split-Path $harnessDll) 'HWIDChecker.dll') -Algorithm SHA256).Hash
        CapturedUtc = [DateTime]::UtcNow.ToString('O')
    } | ConvertTo-Json | Set-Content -LiteralPath (Join-Path $run 'baseline.json') -Encoding utf8NoBOM
    Write-Host "Private captures: $run"

    if ($Golden) {
        # The orchestrator reviews a capture, then writes owner-pc/approved-lines.txt.
        # Each entry: AD-xx<TAB>section<TAB>exact Rust line (without its line terminator).
        # Section and line comparisons are ordinal and exact; no wildcards or trimming.
        # Missing file means no approval. C# repeat checks never consume this allowlist.
        $approvedLinesPath = Join-Path $captureRoot 'approved-lines.txt'
        $approvedLines = Read-ApprovedLines $approvedLinesPath
        Write-Host "Approval ledger: $approved; reviewed lines: $approvedLinesPath ($($approvedLines.Count) entries)."
        $baseline = Join-Path $run 'csharp-report.txt'
        $second = Join-Path $run 'csharp-report-second.txt'
        Invoke-Capture $harnessExe @($baseline) $baseline
        Invoke-Capture $harnessExe @($second) $second
        $csharp = Read-Report $baseline $SectionTitles
        $repeat = Read-Report $second $SectionTitles
        $selected = if ($Sections.Count) { $Sections } else { $SectionTitles }
        $ok = Compare-Lines $csharp.Preamble $repeat.Preamble 'C# repeat / REPORT HEADER' 'C# first' 'C# second'
        foreach ($title in $selected) {
            $a = $csharp.Parts[$title]; $b = $repeat.Parts[$title]
            if ($title -ceq 'USB DEVICES') { $a = Sort-UsbGroups $a; $b = Sort-UsbGroups $b }
            if (-not (Compare-Lines $a $b "C# repeat / $title" 'C# first' 'C# second')) { $ok = $false }
        }
        if ($Sections.Count -eq 0) {
            $dump = Join-Path $run 'rust-report.txt'
            Invoke-Capture $binary @('--dump', $dump) $dump
            $rustReport = Read-Report $dump $SectionTitles
            if (-not (Compare-Lines $csharp.Preamble $rustReport.Preamble 'REPORT HEADER' -ApprovedLines $approvedLines)) { $ok = $false }
        }
        foreach ($title in $selected) {
            if ($Sections.Count) {
                $dump = Join-Path $run ("rust-section-$([Array]::IndexOf($SectionTitles, $title)).txt")
                Invoke-Capture $binary @('--dump', $dump, '--only', $title) $dump
                $rustReport = Read-Report $dump @($title)
                if (-not (Compare-Lines $csharp.Preamble $rustReport.Preamble 'REPORT HEADER' -ApprovedLines $approvedLines)) { $ok = $false }
            }
            $a = $csharp.Parts[$title]; $b = $rustReport.Parts[$title]
            Assert-Implemented $b $title
            if ($title -ceq 'USB DEVICES') { $a = Sort-UsbGroups $a; $b = Sort-UsbGroups $b }
            if (-not (Compare-Lines $a $b $title -ApprovedLines $approvedLines)) { $ok = $false }
        }
        if (-not $ok) { throw 'Golden differences need orchestrator approval; see the private captures and approval ledger.' }
        Write-Host 'Golden comparison passed.'
    } else {
        $flag = if ($Ghosts) { '--ghosts' } elseif ($Logs) { '--logs' } else { '--time' }
        $legacyFile = Join-Path $run 'csharp.txt'
        $rustFile = Join-Path $run 'rust.txt'
        $timeout = if ($Timing) { 4320 } else { 180 }
        Invoke-Capture $harnessExe @($flag, $legacyFile) $legacyFile $timeout
        Invoke-Capture $binary @($flag, $rustFile) $rustFile $timeout
        if ($Ghosts) { Compare-Ghosts $legacyFile $rustFile }
        elseif ($Logs) {
            $a = Read-Utf8 $legacyFile; $b = Read-Utf8 $rustFile
            Assert-Implemented $b 'LOGS'
            if (-not (Compare-Lines $a $b 'LOGS')) { throw 'Log lists differ.' }
            Write-Host 'Log lists match in order.'
        } else {
            $a = Read-Timings $legacyFile; $b = Read-Timings $rustFile
            foreach ($title in $SectionTitles) {
                $delta = if ($a[$title] -gt 0) { '{0:F1}%' -f (100 * ($b[$title] / $a[$title] - 1)) } else { 'n/a' }
                [pscustomobject]@{ Section = $title; 'C# ms' = $a[$title]; 'Rust ms' = $b[$title]; Delta = $delta }
                if ($b[$title] -gt $a[$title] * 1.2) { Write-Warning "$title Rust median is more than 20 percent slower than C#." }
            }
        }
    }
    exit 0
} catch {
    Write-Error $_.Exception.Message -ErrorAction Continue
    exit 1
}
