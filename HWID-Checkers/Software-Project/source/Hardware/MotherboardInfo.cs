using System.Management;
using System.Text;
using HWIDChecker.Services;

namespace HWIDChecker.Hardware;

public class MotherboardInfo : IHardwareInfo
{
    private readonly TextFormattingService _textFormatter;

    public string SectionTitle => "MOTHERBOARD";

    public MotherboardInfo(TextFormattingService textFormatter)
    {
        _textFormatter = textFormatter;
    }

    public string GetInformation()
    {
        var sb = new StringBuilder();
        using var searcher = new ManagementObjectSearcher("SELECT * FROM Win32_BaseBoard");

        foreach (ManagementObject board in searcher.Get())
        {
            _textFormatter.AppendInfoLine(sb, "Manufacturer", board["Manufacturer"]?.ToString() ?? "");
            _textFormatter.AppendInfoLine(sb, "Product", board["Product"]?.ToString() ?? "");
            _textFormatter.AppendInfoLine(sb, "Model", board["Model"]?.ToString() ?? "");
            _textFormatter.AppendInfoLine(sb, "SKU", board["SKU"]?.ToString() ?? "");
            _textFormatter.AppendInfoLine(sb, "SerialNumber", board["SerialNumber"]?.ToString() ?? "");
        }

        return sb.ToString();
    }
}