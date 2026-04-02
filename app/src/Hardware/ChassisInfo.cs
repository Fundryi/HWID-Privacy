using System.Text;
using HWIDChecker.Services;
using HWIDChecker.Services.Win32;

namespace HWIDChecker.Hardware;

public class ChassisInfo : IHardwareInfo
{
    private readonly TextFormattingService _textFormatter;

    public string SectionTitle => "CHASSIS";

    public ChassisInfo(TextFormattingService textFormatter)
    {
        _textFormatter = textFormatter;
    }

    public string GetInformation()
    {
        var sb = new StringBuilder();

        var smbios = FirmwareTable.GetSmbiosData();
        if (smbios == null || string.IsNullOrEmpty(smbios.ChassisManufacturer))
        {
            sb.AppendLine("Chassis information not available.");
            return sb.ToString();
        }

        _textFormatter.AppendInfoLine(sb, "Manufacturer", smbios.ChassisManufacturer ?? "");
        if (!string.IsNullOrEmpty(smbios.ChassisType))
            _textFormatter.AppendInfoLine(sb, "Type", smbios.ChassisType);
        if (!string.IsNullOrEmpty(smbios.ChassisVersion))
            _textFormatter.AppendInfoLine(sb, "Version", smbios.ChassisVersion);
        if (!string.IsNullOrEmpty(smbios.ChassisSerial))
            _textFormatter.AppendInfoLine(sb, "Serial Number", smbios.ChassisSerial);
        if (!string.IsNullOrEmpty(smbios.ChassisAssetTag))
            _textFormatter.AppendInfoLine(sb, "Asset Tag", smbios.ChassisAssetTag);

        return sb.ToString();
    }
}
