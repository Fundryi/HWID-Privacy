using System.Management;
using System.Text;
using HWIDChecker.Services;

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

        // Get System UUID
        using (var searcher = new ManagementObjectSearcher("SELECT UUID FROM Win32_ComputerSystemProduct"))
        {
            foreach (ManagementObject obj in searcher.Get())
            {
                sb.AppendLine($"UUID: {obj["UUID"]}");
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

        return sb.ToString();
    }
}