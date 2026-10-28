# Shared only by the two driver scripts. Does not install or change system policy.
function Invoke-Checked([string]$Exe, [string[]]$Arguments) {
    & $Exe @Arguments
    if ($LASTEXITCODE -ne 0) { throw "$Exe failed (exit $LASTEXITCODE)." }
}

function Get-VisualStudio {
    $vswhere = Join-Path ${env:ProgramFiles(x86)} 'Microsoft Visual Studio/Installer/vswhere.exe'
    if (-not (Test-Path -LiteralPath $vswhere)) { throw 'Install Visual Studio 2026 Build Tools with Desktop development with C++.' }
    $paths = @(& $vswhere -latest -products '*' -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath)
    if ($LASTEXITCODE -ne 0 -or $paths.Count -ne 1) { throw 'Visual Studio x64 C++ Build Tools were not found.' }
    return $paths[0]
}

function Enter-DriverHostEnvironment([string]$VisualStudio) {
    $module = Join-Path $VisualStudio 'Common7/Tools/Microsoft.VisualStudio.DevShell.dll'
    Import-Module $module
    Enter-VsDevShell -VsInstallPath $VisualStudio -SkipAutomaticLocation -DevCmdArguments '-arch=x64 -host_arch=x64' | Out-Null
}
