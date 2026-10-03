# Read-only parity capture; output belongs only in the git-ignored main-checkout golden folder.
$ErrorActionPreference = 'Stop'
$root = 'D:/GIT/HWID-Privacy'
$api = Get-Content -Raw -Encoding UTF8 "$root/app/src/Services/Win32/EventLogApi.cs"
$helper = @'
namespace Wp12Capture {
    public static class ReadOnly {
        public static string[] Additional(string[] standard) {
            var known = new System.Collections.Generic.HashSet<string>(standard, System.StringComparer.OrdinalIgnoreCase);
            var candidates = new System.Collections.Generic.List<string>();
            foreach (var raw in HWIDChecker.Services.Win32.EventLogApi.EnumerateChannels()) {
                var name = (raw ?? "").Trim();
                if (name.Length != 0 && known.Add(name)) candidates.Add(name);
            }
            var found = new System.Collections.Concurrent.ConcurrentBag<string>();
            System.Threading.Tasks.Parallel.ForEach(candidates,
                new System.Threading.Tasks.ParallelOptions { MaxDegreeOfParallelism = 12 },
                name => {
                    if (HWIDChecker.Services.Win32.EventLogApi.IsChannelEnabled(name) != false) found.Add(name);
                });
            var result = new System.Collections.Generic.List<string>(found);
            result.Sort(System.StringComparer.OrdinalIgnoreCase);
            return result.ToArray();
        }
    }
}
'@
Add-Type -TypeDefinition ("using System.Collections.Generic;`n" + $api + "`n" + $helper)
$source = Get-Content -Raw -Encoding UTF8 "$root/app/src/Services/EventLogCleaningService.cs"
$block = [regex]::Match($source, '(?s)private readonly string\[\] StandardEventLogs.*?private record ProcessResult').Value
$standard = @([regex]::Matches($block, '"([^"\r\n]+)"') | ForEach-Object { $_.Groups[1].Value })
$timer = [Diagnostics.Stopwatch]::StartNew()
$additional = @([Wp12Capture.ReadOnly]::Additional([string[]]$standard))
$timer.Stop()
$directory = "$root/app/rust/golden/wp-12"
[IO.Directory]::CreateDirectory($directory) | Out-Null
$utf8 = [Text.UTF8Encoding]::new($false)
[IO.File]::WriteAllText("$directory/csharp-logs-nonadmin.txt", (($standard + $additional) -join "`r`n") + "`r`n", $utf8)
[IO.File]::WriteAllText("$directory/csharp-discovery-timing.txt", "standard=$($standard.Count) additional=$($additional.Count) elapsed_ms=$($timer.ElapsedMilliseconds)`r`n", $utf8)
Write-Output "standard=$($standard.Count) additional=$($additional.Count) elapsed_ms=$($timer.ElapsedMilliseconds)"
