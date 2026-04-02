using System;
using System.Management;
using System.Text;
using HWIDChecker.Services;
using Microsoft.Win32;

namespace HWIDChecker.Hardware;

public class SystemInfo : IHardwareInfo
{
    private readonly TextFormattingService textFormatter;

    public string SectionTitle => "SYSTEM INFORMATION";

    public SystemInfo(TextFormattingService textFormatter = null)
    {
        this.textFormatter = textFormatter;
    }

    public string GetInformation()
    {
        var sb = new StringBuilder();

        // Get Windows Product Key
        using (var searcher = new ManagementObjectSearcher("SELECT * FROM SoftwareLicensingService"))
        {
            foreach (ManagementObject obj in searcher.Get())
            {
                var key = obj["OA3xOriginalProductKey"]?.ToString();
                if (!string.IsNullOrEmpty(key))
                {
                    sb.AppendLine($"Windows Product Key: {key}");
                }
                else
                {
                    sb.AppendLine("Activation Status: Not activated or using Volume License");
                }
            }
        }

        // Get OS Serial Number
        using (var searcher = new ManagementObjectSearcher("SELECT SerialNumber FROM Win32_OperatingSystem"))
        {
            foreach (ManagementObject obj in searcher.Get())
            {
                sb.AppendLine($"Serial Number (Product ID): {obj["SerialNumber"]}");
            }
        }

        // Machine GUID
        var machineGuid = Registry.GetValue(
            @"HKEY_LOCAL_MACHINE\SOFTWARE\Microsoft\Cryptography",
            "MachineGuid",
            null)?.ToString();
        if (!string.IsNullOrEmpty(machineGuid))
        {
            sb.AppendLine($"Machine GUID: {machineGuid}");
        }

        // Hardware Profile GUID
        var hardwareProfileGuid = Registry.GetValue(
            @"HKEY_LOCAL_MACHINE\SYSTEM\CurrentControlSet\Control\IDConfigDB\Hardware Profiles\0001",
            "HwProfileGuid",
            null)?.ToString();
        if (!string.IsNullOrEmpty(hardwareProfileGuid))
        {
            sb.AppendLine($"Hardware Profile GUID: {hardwareProfileGuid}");
        }

        // Windows Install Date
        var installDateValue = Registry.GetValue(
            @"HKEY_LOCAL_MACHINE\SOFTWARE\Microsoft\Windows NT\CurrentVersion",
            "InstallDate",
            null);
        if (installDateValue != null && long.TryParse(installDateValue.ToString(), out long installDateSeconds))
        {
            var installDate = DateTimeOffset.FromUnixTimeSeconds(installDateSeconds).LocalDateTime;
            sb.AppendLine($"Install Date: {installDate:yyyy-MM-dd HH:mm:ss}");
        }

        return sb.ToString();
    }
}
