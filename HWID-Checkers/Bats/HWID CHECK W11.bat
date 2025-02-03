@echo off
setlocal enabledelayedexpansion

>nul 2>&1 "%SYSTEMROOT%\system32\cacls.exe" "%SYSTEMROOT%\system32\config\system"
if '%errorlevel%' NEQ '0' (
    echo Requesting administrative privileges...
    goto UACPrompt
) else ( goto gotAdmin )

:UACPrompt
    echo Set UAC = CreateObject^("Shell.Application"^) > "%temp%\getadmin.vbs"
    echo UAC.ShellExecute "%~s0", "", "", "runas", 1 >> "%temp%\getadmin.vbs"
    "%temp%\getadmin.vbs"
    exit /B

:gotAdmin
    if exist "%temp%\getadmin.vbs" ( del "%temp%\getadmin.vbs" )
    pushd "%CD%"
    CD /D "%~dp0"

if "%1"=="export" goto ExportData

start "" /min cmd /c "%~f0" export

powershell -command "&{$H=get-host;$W=$H.ui.rawui;$B=$W.buffersize;$B.width=120;$B.height=9999;$W.buffersize=$B;$S=$W.windowsize;$S.width=120;$S.height=50;$W.windowsize=$S}"

title Comprehensive HWID Checker (Display)

:start
cls
call :DisplayInfo
echo.
echo Do you want to save an Export file for later?
echo Press any key to save, or close this window to exit without saving.
pause >nul

:: Generate timestamp for file name
for /f "tokens=2 delims==" %%a in ('wmic OS Get localdatetime /value') do set "dt=%%a"
set "YYYY=%dt:~0,4%"
set "MM=%dt:~4,2%"
set "DD=%dt:~6,2%"
set "HH=%dt:~8,2%"
set "Min=%dt:~10,2%"
set "Sec=%dt:~12,2%"

set "datestamp=%DD%%MM%%YYYY:~2,2%"
set "timestamp=%HH%%Min%%Sec%"
set "fullstamp=%datestamp%_%timestamp%"

set "export_file=%~dp0HWID_CHECK_EXPORT_%fullstamp%.txt"

:: Start export process
echo Exporting data to %export_file%...
call :ExportData
echo Export complete.
echo.
echo Press any key to refresh the information...
pause >nul
goto start

:DisplayInfo

:: Enable ANSI escape sequences
set "ESC="

set "PURPLE=%ESC%[35m"
set "BRIGHTCYAN=%ESC%[96m"
set "LIGHTBLUE=%ESC%[33m"
set "BRIGHTRED=%ESC%[91m"
set "RESET=%ESC%[0m"


echo %PURPLE%                                          ================================
echo %BRIGHTCYAN%                                             Comprehensive HWID Checker

echo %PURPLE%======================================================================================================================
echo %BRIGHTCYAN%                                                    DISK DRIVES
echo %PURPLE%======================================================================================================================%LIGHTBLUE%
powershell -Command "Get-PhysicalDisk | ForEach-Object { '{0}  {1}  {2}' -f $_.DeviceID, $_.Model, $_.SerialNumber }"
powershell -Command "Get-Partition | ForEach-Object { '{0} {1}' -f $_.DriveLetter, $_.AccessPath }"

echo %PURPLE%======================================================================================================================
echo %BRIGHTCYAN%                                                       CPU
echo %PURPLE%======================================================================================================================%BRIGHTRED%
powershell -Command "Get-CimInstance Win32_Processor | ForEach-Object { '{0} {1}' -f $_.ProcessorId, $_.Name }"

echo %PURPLE%======================================================================================================================
echo %BRIGHTCYAN%                                                  SYSTEM INFORMATION
echo %PURPLE%======================================================================================================================%LIGHTBLUE%
powershell -Command "Get-CimInstance Win32_ComputerSystemProduct | ForEach-Object { 'UUID: {0}' -f $_.UUID }"
powershell -Command "Get-CimInstance SoftwareLicensingService | ForEach-Object { if ($_.OA3xOriginalProductKey) { 'Windows Product Key: {0}' -f $_.OA3xOriginalProductKey } else { 'Activation Status: Not activated' } }"
powershell -Command "Get-CimInstance Win32_OperatingSystem | ForEach-Object { 'Serial Number (Product ID): {0}' -f $_.SerialNumber }"

echo %PURPLE%======================================================================================================================
echo %BRIGHTCYAN%                                                    MOTHERBOARD
echo %PURPLE%======================================================================================================================%BRIGHTRED%
powershell -Command "Get-CimInstance Win32_BaseBoard | ForEach-Object { '{0}  {1}  {2}  {3}' -f $_.Manufacturer, $_.Product, $_.SerialNumber, $_.Version }"

echo %PURPLE%======================================================================================================================
echo %BRIGHTCYAN%                                                     (SM)BIOS
echo %PURPLE%======================================================================================================================%LIGHTBLUE%
powershell -Command "Get-CimInstance Win32_BIOS | ForEach-Object { '{0}  {1}  {2}' -f $_.Manufacturer, $_.SerialNumber, $_.SMBIOSBIOSVersion }"
echo.
powershell -Command "Get-CimInstance Win32_ComputerSystemProduct | ForEach-Object { '{0,-20} {1,-40} {2,-35} {3}' -f $_.IdentifyingNumber, $_.UUID, $_.Vendor, $_.Version }"

echo %PURPLE%======================================================================================================================
echo %BRIGHTCYAN%                                                    RAM MODULES
echo %PURPLE%======================================================================================================================%BRIGHTRED%
powershell -Command "Get-CimInstance Win32_PhysicalMemory | ForEach-Object { '{0}  {1}' -f $_.DeviceLocator, $_.SerialNumber }"

echo %PURPLE%======================================================================================================================
echo %BRIGHTCYAN%                                                    TPM MODULES
echo %PURPLE%======================================================================================================================%LIGHTBLUE%
powershell -Command "$tpm = Get-Tpm; if ($tpm.TpmPresent -and $tpm.TpmEnabled) { Write-Output 'TPM ENABLED'; Write-Output ('Manufacturer ID: ' + $tpm.ManufacturerID); Write-Output ('Manufacturer Version: ' + $tpm.ManufacturerVersion); Write-Output ('Manufacturer Version Info: ' + ($tpm.ManufacturerVersionInfo -or 'N/A')); Write-Output ('Physical Presence Version Info: ' + ($tpm.PhysicalPresenceVersion -or 'N/A')); $tpmUniqueData = [string]$tpm.ManufacturerID + [string]$tpm.ManufacturerVersion + [string]($tpm.PhysicalPresenceVersion -or '0'); $tpmBytes = [System.Text.Encoding]::UTF8.GetBytes($tpmUniqueData); $md5 = ([System.Security.Cryptography.MD5]::Create().ComputeHash($tpmBytes)) -join ''; $sha1 = ([System.Security.Cryptography.SHA1]::Create().ComputeHash($tpmBytes)) -join ''; $sha256 = ([System.Security.Cryptography.SHA256]::Create().ComputeHash($tpmBytes)) -join ''; Write-Output ('MD5: ' + ($md5 -replace '(..)', '$1 ').Trim()); Write-Output ('SHA1: ' + ($sha1 -replace '(..)', '$1 ').Trim()); Write-Output ('SHA256: ' + ($sha256 -replace '(..)', '$1 ').Trim()); } else { Write-Output 'TPM DISABLED' }"

echo %PURPLE%======================================================================================================================
echo %BRIGHTCYAN%                                                     GPU INFO
echo %PURPLE%======================================================================================================================%BRIGHTRED%
nvidia-smi -L > nul 2>&1
if %errorlevel% equ 0 (
    nvidia-smi -L
) else (
    powershell -Command "$gpus = Get-CimInstance Win32_VideoController; foreach ($gpu in $gpus) { Write-Output ('Name: ' + $gpu.Name + ', PNPDeviceID: ' + $gpu.PNPDeviceID) }"
)

echo %PURPLE%======================================================================================================================
echo %BRIGHTCYAN%                                                    USB DEVICES
echo %PURPLE%======================================================================================================================%LIGHTBLUE%
powershell -Command "$usbDevices = Get-WmiObject Win32_USBControllerDevice | ForEach-Object { [wmi]($_.Dependent) }; foreach ($device in $usbDevices) { $pnpEntity = Get-WmiObject Win32_PnPEntity -Filter \"DeviceID='$($device.DeviceID -replace '\\', '\\')'\"; if ($pnpEntity.PNPDeviceID -match '.*\\(.+)') { $serial = $matches[1]; if ($serial -ne '0000000000000000' -and $serial -notmatch '[&.{}]') { Write-Output (\"Device: $($pnpEntity.Name)`nDescription: $($pnpEntity.Description)`nType: $($pnpEntity.PNPClass)`nSerial: $serial`n\") } } }"

echo %PURPLE%======================================================================================================================
echo %BRIGHTCYAN%                                                  MONITOR INFORMATION
echo %PURPLE%======================================================================================================================%BRIGHTRED%
powershell -Command "$monitors = Get-WmiObject WmiMonitorID -Namespace root\wmi; $count = $monitors.Count; Write-Output \"$count monitors found.`n\"; $separator = '-' * 40; Write-Output $separator; for ($i = 0; $i -lt $count; $i++ ) { $monitor = $monitors[$i]; $manufacturer = [System.Text.Encoding]::ASCII.GetString($monitor.ManufacturerName).Trim(0); $name = [System.Text.Encoding]::ASCII.GetString($monitor.UserFriendlyName).Trim(0); $serial = [System.Text.Encoding]::ASCII.GetString($monitor.SerialNumberID).Trim(0); Write-Output \"Monitor $($i+1):`nActive: $($monitor.Active)`nInstanceName: $($monitor.InstanceName)`nManufacturerName: $manufacturer`nProductCodeID: $([System.Text.Encoding]::ASCII.GetString($monitor.ProductCodeID).Trim(0))`nSerialNumberID: $serial`nUserFriendlyName: $name`nUserFriendlyNameLength: $($monitor.UserFriendlyNameLength)`nWeekOfManufacture: $($monitor.WeekOfManufacture)`nYearOfManufacture: $($monitor.YearOfManufacture)\"; if ($i -lt $count - 1 ) { Write-Output $separator } }; Write-Output $separator"

echo %PURPLE%======================================================================================================================
echo %BRIGHTCYAN%                                                NETWORK ADAPTERS (NIC's)
echo %PURPLE%======================================================================================================================%LIGHTBLUE%
powershell -Command "Get-CimInstance Win32_NetworkAdapter | Where-Object { $_.MACAddress -ne $null } | ForEach-Object { '{0}  {1}  {2}' -f $_.MACAddress, $_.Name, $_.DeviceID }"

echo %PURPLE%======================================================================================================================
echo %BRIGHTCYAN%                                                     ARP INFO/CACHE
echo %PURPLE%======================================================================================================================%BRIGHTRED%
arp -a
echo %PURPLE%======================================================================================================================%RESET%
goto :eof

:ExportData
:: Start capturing output
echo                                           ================================ > "%export_file%"
echo                                              Comprehensive HWID Checker >> "%export_file%"

echo ====================================================================================================================== >> "%export_file%"
echo                                                     DISK DRIVES >> "%export_file%"
echo ====================================================================================================================== >> "%export_file%"
powershell -Command "Get-WmiObject Win32_DiskDrive | ForEach-Object { '{0}  {1}  {2}' -f $_.DeviceID, $_.Model, $_.SerialNumber }" >> "%export_file%"
powershell -Command "wmic logicaldisk get deviceid,volumeserialnumber | ForEach-Object { $_.Trim() } | Where-Object { $_ } | Select-Object -Skip 1 | ForEach-Object { $parts = $_ -split '\\s+'; '{0} {1}' -f $parts[0], $parts[1] }" >> "%export_file%"

echo ====================================================================================================================== >> "%export_file%"
echo                                                        CPU >> "%export_file%"
echo ====================================================================================================================== >> "%export_file%"
powershell -Command "wmic cpu get processorid,serialnumber | ForEach-Object { $_.Trim() } | Where-Object { $_ }" >> "%export_file%"

echo ====================================================================================================================== >> "%export_file%"
echo                                                   SYSTEM INFORMATION >> "%export_file%"
echo ====================================================================================================================== >> "%export_file%"
powershell -Command "$uuid = (Get-WmiObject -Class Win32_ComputerSystemProduct).UUID; Write-Output ('UUID: ' + $uuid)" >> "%export_file%"
powershell -Command "$key = (Get-WmiObject -Class SoftwareLicensingService).OA3xOriginalProductKey; if ($key) { Write-Output ('Windows Product Key: ' + $key) } else { $status = (Get-WmiObject -Class SoftwareLicensingService).LicenseStatus; if ($status -eq 1) { Write-Output 'Activation Status: Activated (possibly by KMS or Volume License)' } else { Write-Output 'Activation Status: Not activated' } }" >> "%export_file%"
powershell -Command "Write-Output ('Serial Number (Product ID): ' + (Get-WmiObject -Class Win32_OperatingSystem).SerialNumber)" >> "%export_file%"

echo ====================================================================================================================== >> "%export_file%"
echo                                                     MOTHERBOARD >> "%export_file%"
echo ====================================================================================================================== >> "%export_file%"
powershell -Command "wmic baseboard get manufacturer,product,serialnumber,version | ForEach-Object { $_.Trim() } | Where-Object { $_ }" >> "%export_file%"

echo ====================================================================================================================== >> "%export_file%"
echo                                                      (SM)BIOS >> "%export_file%"
echo ====================================================================================================================== >> "%export_file%"
powershell -Command "wmic bios get manufacturer,serialnumber,version | ForEach-Object { $_.Trim() } | Where-Object { $_ }"  >> "%export_file%"
echo. >> "%export_file%"
powershell -Command "wmic path win32_computersystemproduct get identifyingnumber,uuid,vendor,version | ForEach-Object { $_.Trim() } | Where-Object { $_ }" >> "%export_file%"

echo ====================================================================================================================== >> "%export_file%"
echo                                                     RAM MODULES >> "%export_file%"
echo ====================================================================================================================== >> "%export_file%"
powershell -Command "wmic memorychip get devicelocator,serialnumber | ForEach-Object { $_.Trim() } | Where-Object { $_ }" >> "%export_file%"

echo ====================================================================================================================== >> "%export_file%"
echo                                                     TPM MODULES >> "%export_file%"
echo ====================================================================================================================== >> "%export_file%"
powershell -Command "$tpm = Get-Tpm; if ($tpm.TpmPresent -and $tpm.TpmEnabled) { Write-Output 'TPM ENABLED'; Write-Output ('Manufacturer ID: ' + $tpm.ManufacturerID); Write-Output ('Manufacturer Version: ' + $tpm.ManufacturerVersion); Write-Output ('Manufacturer Version Info: ' + ($tpm.ManufacturerVersionInfo -or 'N/A')); Write-Output ('Physical Presence Version Info: ' + ($tpm.PhysicalPresenceVersion -or 'N/A')); $tpmUniqueData = [string]$tpm.ManufacturerID + [string]$tpm.ManufacturerVersion + [string]($tpm.PhysicalPresenceVersion -or '0'); $tpmBytes = [System.Text.Encoding]::UTF8.GetBytes($tpmUniqueData); $md5 = ([System.Security.Cryptography.MD5]::Create().ComputeHash($tpmBytes)) -join ''; $sha1 = ([System.Security.Cryptography.SHA1]::Create().ComputeHash($tpmBytes)) -join ''; $sha256 = ([System.Security.Cryptography.SHA256]::Create().ComputeHash($tpmBytes)) -join ''; Write-Output ('MD5: ' + ($md5 -replace '(..)', '$1 ').Trim()); Write-Output ('SHA1: ' + ($sha1 -replace '(..)', '$1 ').Trim()); Write-Output ('SHA256: ' + ($sha256 -replace '(..)', '$1 ').Trim()); } else { Write-Output 'TPM DISABLED' }" >> "%export_file%"

echo ====================================================================================================================== >> "%export_file%"
echo                                                      GPU INFO >> "%export_file%"
echo ====================================================================================================================== >> "%export_file%"
nvidia-smi -L > nul 2>&1
if %errorlevel% equ 0 ( 
    nvidia-smi -L  >> "%export_file%"
) else (
    powershell -Command "wmic path win32_videocontroller get name,pnpdeviceid | ForEach-Object { $_.Trim() } | Where-Object { $_ }"  >> "%export_file%"
)

echo ====================================================================================================================== >> "%export_file%"
echo                                                     USB DEVICES >> "%export_file%"
echo ====================================================================================================================== >> "%export_file%"
powershell -Command "$usbDevices = Get-WmiObject Win32_USBControllerDevice | ForEach-Object { [wmi]($_.Dependent) }; foreach ($device in $usbDevices) { $pnpEntity = Get-WmiObject Win32_PnPEntity -Filter \"DeviceID='$($device.DeviceID -replace '\\', '\\')'\"; if ($pnpEntity.PNPDeviceID -match '.*\\(.+)') { $serial = $matches[1]; if ($serial -ne '0000000000000000' -and $serial -notmatch '[&.{}]') { Write-Output (\"Device: $($pnpEntity.Name)`nDescription: $($pnpEntity.Description)`nType: $($pnpEntity.PNPClass)`nSerial: $serial`n\") } } }" >> "%export_file%"

echo ====================================================================================================================== >> "%export_file%"
echo                                                   MONITOR INFORMATION >> "%export_file%"
echo ====================================================================================================================== >> "%export_file%"
powershell -Command "$monitors = Get-WmiObject WmiMonitorID -Namespace root\wmi; $count = $monitors.Count; Write-Output \"$count monitors found.`n\"; $separator = '-' * 40; Write-Output $separator; for ($i = 0; $i -lt $count; $i++ ) { $monitor = $monitors[$i]; $manufacturer = [System.Text.Encoding]::ASCII.GetString($monitor.ManufacturerName).Trim(0); $name = [System.Text.Encoding]::ASCII.GetString($monitor.UserFriendlyName).Trim(0); $serial = [System.Text.Encoding]::ASCII.GetString($monitor.SerialNumberID).Trim(0); Write-Output \"Monitor $($i+1):`nActive: $($monitor.Active)`nInstanceName: $($monitor.InstanceName)`nManufacturerName: $manufacturer`nProductCodeID: $([System.Text.Encoding]::ASCII.GetString($monitor.ProductCodeID).Trim(0))`nSerialNumberID: $serial`nUserFriendlyName: $name`nUserFriendlyNameLength: $($monitor.UserFriendlyNameLength)`nWeekOfManufacture: $($monitor.WeekOfManufacture)`nYearOfManufacture: $($monitor.YearOfManufacture)\"; if ($i -lt $count - 1 ) { Write-Output $separator } }; Write-Output $separator" >> "%export_file%"

echo ====================================================================================================================== >> "%export_file%"
echo                                                 NETWORK ADAPTERS (NIC's) >> "%export_file%"
echo ====================================================================================================================== >> "%export_file%"
powershell -Command "wmic nic where 'MACAddress is not null' get macaddress,name,deviceid | ForEach-Object { $_.Trim() } | Where-Object { $_ }"  >> "%export_file%"

echo ====================================================================================================================== >> "%export_file%"
echo                                                      ARP INFO/CACHE >> "%export_file%"
echo ====================================================================================================================== >> "%export_file%"
arp -a >> "%export_file%"
echo ====================================================================================================================== >> "%export_file%"
goto :eof