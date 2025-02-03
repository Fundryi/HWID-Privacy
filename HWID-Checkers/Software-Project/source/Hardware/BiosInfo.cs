using System.Management;
using System.Text;
using HWIDChecker.Services;

namespace HWIDChecker.Hardware;

public class BiosInfo : IHardwareInfo
{
    private readonly TextFormattingService _textFormatter;

    public string SectionTitle => "(SM)BIOS";

    public BiosInfo(TextFormattingService textFormatter)
    {
        _textFormatter = textFormatter;
    }

    public string GetInformation()
    {
        var sb = new StringBuilder();
        var info = new Dictionary<string, string>();

        // Collect BIOS Information
        using (var searcher = new ManagementObjectSearcher("SELECT * FROM Win32_BIOS"))
        {
            foreach (ManagementObject bios in searcher.Get())
            {
                info["Manufacturer"] = bios["Manufacturer"]?.ToString() ?? "";
                info["Version"] = bios["Version"]?.ToString() ?? "";
                info["SerialNumber"] = bios["SerialNumber"]?.ToString() ?? "";
            }
        }

        // Collect System Product Information
        using (var searcher = new ManagementObjectSearcher("SELECT * FROM Win32_ComputerSystemProduct"))
        {
            foreach (ManagementObject product in searcher.Get())
            {
                info["Vendor"] = product["Vendor"]?.ToString() ?? "";
                info["UUID"] = product["UUID"]?.ToString() ?? "";
                info["IdentifyingNumber"] = product["IdentifyingNumber"]?.ToString() ?? "";
                // Version marked as (CAN BE REMOVED) is not included here
            }
        }

        // Format information in desired order
        _textFormatter.AppendInfoLine(sb, "Manufacturer", info["Manufacturer"]);
        _textFormatter.AppendInfoLine(sb, "Vendor", info["Vendor"]);
        _textFormatter.AppendInfoLine(sb, "Version", info["Version"]);
        _textFormatter.AppendInfoLine(sb, "UUID", info["UUID"]);
        _textFormatter.AppendInfoLine(sb, "IdentifyingNumber", info["IdentifyingNumber"]);
        _textFormatter.AppendInfoLine(sb, "SerialNumber", info["SerialNumber"]);

        return sb.ToString();
    }
}
