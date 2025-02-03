using System.Management;
using System.Text;
using HWIDChecker.Services;

namespace HWIDChecker.Hardware;

public class UsbInfo : IHardwareInfo
{
    private readonly TextFormattingService _textFormatter;

    public string SectionTitle => "USB DEVICES";

    public UsbInfo(TextFormattingService textFormatter)
    {
        _textFormatter = textFormatter;
    }

    public string GetInformation()
    {
        var sb = new StringBuilder();
        var devices = new List<(string Name, string Description, string Type, string Serial)>();

        try
        {
            using var searcher = new ManagementObjectSearcher(@"SELECT * FROM Win32_PnPEntity WHERE PNPDeviceID LIKE 'USB%'");
            foreach (ManagementObject device in searcher.Get())
            {
                string pnpDeviceID = device["PNPDeviceID"]?.ToString() ?? "";
                if (string.IsNullOrEmpty(pnpDeviceID) || !pnpDeviceID.Contains("\\")) continue;

                string serial = pnpDeviceID.Split('\\').Last();
                if (serial != "0000000000000000" && !serial.Contains("&") && !serial.Contains(".") && !serial.Contains("{"))
                {
                    string name = device["Name"]?.ToString() ?? "";
                    string description = device["Description"]?.ToString() ?? "";
                    string pnpClass = device["PNPClass"]?.ToString() ?? "USB";

                    devices.Add((name, description, pnpClass, serial));
                }
            }

            // Format devices with separators
            var deviceInfos = devices.Select(d => new[]
            {
                ("Device", d.Name),
                ("Description", d.Description),
                ("Type", d.Type),
                ("Serial", d.Serial)
            }).ToList();

            _textFormatter.AppendDeviceGroup(sb, deviceInfos);
        }
        catch
        {
            _textFormatter.AppendInfoLine(sb, "Error", "Unable to retrieve USB information");
        }

        return sb.ToString();
    }
}