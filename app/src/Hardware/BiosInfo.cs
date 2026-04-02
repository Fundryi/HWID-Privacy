using System.Management;
using System.Text;
using HWIDChecker.Services;
using HWIDChecker.Services.Win32;

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

        // Try raw SMBIOS first (Type 0 + Type 1)
        var smbios = FirmwareTable.GetSmbiosData();

        // Collect BIOS Information from WMI (always, for fields SMBIOS may not have)
        using (var searcher = new ManagementObjectSearcher("SELECT * FROM Win32_BIOS"))
        {
            foreach (ManagementObject bios in searcher.Get())
            {
                info["Manufacturer"] = bios["Manufacturer"]?.ToString() ?? "";
                info["Version"] = bios["Version"]?.ToString() ?? "";
                info["SMBIOSBIOSVersion"] = bios["SMBIOSBIOSVersion"]?.ToString() ?? "";
                info["SerialNumber"] = bios["SerialNumber"]?.ToString() ?? "";
            }
        }

        // Collect System Product Information from WMI
        using (var searcher = new ManagementObjectSearcher("SELECT * FROM Win32_ComputerSystemProduct"))
        {
            foreach (ManagementObject product in searcher.Get())
            {
                info["Vendor"] = product["Vendor"]?.ToString() ?? "";
                info["UUID"] = product["UUID"]?.ToString() ?? "";
                info["IdentifyingNumber"] = product["IdentifyingNumber"]?.ToString() ?? "";
            }
        }

        // Prefer SMBIOS data where available, fall back to WMI
        _textFormatter.AppendInfoLine(sb, "Manufacturer",
            !string.IsNullOrEmpty(smbios?.BiosVendor) ? smbios.BiosVendor : info.GetValueOrDefault("Manufacturer", ""));
        _textFormatter.AppendInfoLine(sb, "Vendor", info.GetValueOrDefault("Vendor", ""));
        _textFormatter.AppendInfoLine(sb, "Version",
            !string.IsNullOrEmpty(smbios?.BiosVersion) ? smbios.BiosVersion : info.GetValueOrDefault("Version", ""));
        _textFormatter.AppendInfoLine(sb, "SMBIOS Version", info.GetValueOrDefault("SMBIOSBIOSVersion", ""));

        if (!string.IsNullOrEmpty(smbios?.BiosReleaseDate))
            _textFormatter.AppendInfoLine(sb, "Release Date", smbios.BiosReleaseDate);

        _textFormatter.AppendInfoLine(sb, "UUID",
            !string.IsNullOrEmpty(smbios?.SystemUuid) ? smbios.SystemUuid : info.GetValueOrDefault("UUID", ""));
        _textFormatter.AppendInfoLine(sb, "IdentifyingNumber", info.GetValueOrDefault("IdentifyingNumber", ""));
        _textFormatter.AppendInfoLine(sb, "SerialNumber", info.GetValueOrDefault("SerialNumber", ""));

        // SMBIOS-only fields (Type 1 - System Information)
        if (smbios != null)
        {
            if (!string.IsNullOrEmpty(smbios.SystemManufacturer))
                _textFormatter.AppendInfoLine(sb, "System Manufacturer", smbios.SystemManufacturer);
            if (!string.IsNullOrEmpty(smbios.SystemProduct))
                _textFormatter.AppendInfoLine(sb, "System Product", smbios.SystemProduct);
            if (!string.IsNullOrEmpty(smbios.SystemSerial))
                _textFormatter.AppendInfoLine(sb, "System Serial", smbios.SystemSerial);
            if (!string.IsNullOrEmpty(smbios.SystemSku))
                _textFormatter.AppendInfoLine(sb, "System SKU", smbios.SystemSku);
            if (!string.IsNullOrEmpty(smbios.SystemFamily))
                _textFormatter.AppendInfoLine(sb, "System Family", smbios.SystemFamily);
        }

        return sb.ToString();
    }
}
