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
            var serialNumber = ram["SerialNumber"]?.ToString() ?? "";

            _textFormatter.AppendCombinedInfoLine(sb,
                ("DeviceLocator", deviceLocator),
                ("SerialNumber", serialNumber));
        }

        return sb.ToString();
    }
}