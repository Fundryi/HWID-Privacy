using System.Management;
using System.Text;
using HWIDChecker.Services;

namespace HWIDChecker.Hardware;

public class RamInfo : IHardwareInfo
{
    private readonly TextFormattingService _textFormatter;

    public string SectionTitle => "RAM MODULES";

    public RamInfo(TextFormattingService textFormatter)
    {
        _textFormatter = textFormatter;
    }

    public string GetInformation()
    {
        var sb = new StringBuilder();
        using var searcher = new ManagementObjectSearcher("SELECT * FROM Win32_PhysicalMemory");

        foreach (ManagementObject ram in searcher.Get())
        {
            var deviceLocator = ram["DeviceLocator"]?.ToString() ?? "";
            var manufacturer = ram["Manufacturer"]?.ToString() ?? "";
            var partNumber = ram["PartNumber"]?.ToString() ?? "";
            var serialNumber = ram["SerialNumber"]?.ToString() ?? "";
            
            // Convert capacity from bytes to GB and format with 0 decimal places
            var capacityBytes = Convert.ToUInt64(ram["Capacity"] ?? 0);
            var capacityGB = capacityBytes / (1024.0 * 1024.0 * 1024.0);
            var capacityFormatted = $"{capacityGB:N0} GB";

            _textFormatter.AppendCombinedInfoLine(sb,
                ("DeviceLocator", deviceLocator),
                ("Manufacturer", manufacturer),
                ("PartNumber", partNumber),
                ("Capacity", capacityFormatted),
                ("SerialNumber", serialNumber));
        }

        return sb.ToString();
    }
}