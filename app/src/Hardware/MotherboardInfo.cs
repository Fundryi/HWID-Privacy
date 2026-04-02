using System.Management;
using System.Text;
using HWIDChecker.Services;
using HWIDChecker.Services.Win32;

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

        // Try raw SMBIOS first (Type 2 - Baseboard)
        var smbios = FirmwareTable.GetSmbiosData();
        if (smbios != null && !string.IsNullOrEmpty(smbios.BoardManufacturer))
        {
            _textFormatter.AppendInfoLine(sb, "Manufacturer", smbios.BoardManufacturer ?? "");
            _textFormatter.AppendInfoLine(sb, "Product", smbios.BoardProduct ?? "");
            _textFormatter.AppendInfoLine(sb, "Version", smbios.BoardVersion ?? "");
            _textFormatter.AppendInfoLine(sb, "SerialNumber", smbios.BoardSerial ?? "");

            // Fields WMI doesn't expose
            if (!string.IsNullOrEmpty(smbios.BoardAssetTag))
                _textFormatter.AppendInfoLine(sb, "Asset Tag", smbios.BoardAssetTag);
            if (!string.IsNullOrEmpty(smbios.BoardLocation))
                _textFormatter.AppendInfoLine(sb, "Location", smbios.BoardLocation);

            _textFormatter.AppendInfoLine(sb, "Source", "SMBIOS (direct)");
            return sb.ToString();
        }

        // Fallback to WMI
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
