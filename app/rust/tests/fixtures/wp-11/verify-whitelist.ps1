# Runs the unchanged legacy converter in memory, with fabricated fixture data only.
# No app launch, whitelist save/reset, native call, or source edit.
#requires -Version 7.0
[CmdletBinding()]
param([string]$LegacyRoot = (Join-Path $PSScriptRoot '../../../../src'))
$ErrorActionPreference = 'Stop'
$sources = @(
    Get-Content -Raw -LiteralPath (Join-Path $LegacyRoot 'Services/DeviceWhitelistService.cs')
    Get-Content -Raw -LiteralPath (Join-Path $LegacyRoot 'Services/Models/DeviceDetail.cs')
)
$usings = @($sources | ForEach-Object {
    [regex]::Matches($_, '(?m)^using .+;\r?$') | ForEach-Object { $_.Value.Trim() }
}) | Sort-Object -Unique
$bodies = $sources | ForEach-Object { [regex]::Replace($_, '(?m)^using .+;\r?\n', '') }
# The converter only constructs a default SP_DEVINFO_DATA. No native ABI is exercised.
$stub = @'
namespace HWIDChecker.Services.Win32 {
    public static class SetupApi { public struct SP_DEVINFO_DATA {} }
}
public static class Wp11Compatibility {
    public static string RoundTrip(string json) {
        var options = new System.Text.Json.JsonSerializerOptions { WriteIndented = true };
        options.Converters.Add(new HWIDChecker.Services.DeviceDetailConverter());
        var devices = System.Text.Json.JsonSerializer.Deserialize<
            System.Collections.Generic.List<HWIDChecker.Services.Models.DeviceDetail>>(json, options);
        return System.Text.Json.JsonSerializer.Serialize(devices, options);
    }
}
'@
Add-Type -TypeDefinition (($usings -join "`n") + "`n" + ($bodies -join "`n") + "`n" + $stub)
$legacy = Get-Content -Raw -LiteralPath (Join-Path $PSScriptRoot 'csharp-whitelist.json')
$rust = Get-Content -Raw -LiteralPath (Join-Path $PSScriptRoot 'rust-whitelist.json')
$generated = [Wp11Compatibility]::RoundTrip($rust)
# Whitespace outside strings is not part of AD-30's parsed-content contract.
if (($generated -replace "`r`n", "`n").TrimEnd() -cne ($legacy -replace "`r`n", "`n").TrimEnd()) { throw 'Legacy converter output differs from the C# fixture.' }
if ([Wp11Compatibility]::RoundTrip($legacy).TrimEnd() -cne $generated.TrimEnd()) {
    throw 'C# fixture round trip failed.'
}
Write-Output 'PASS: unchanged C# converter reads Rust JSON and reproduces the C# fixture exactly.'
