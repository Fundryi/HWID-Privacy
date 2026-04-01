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
        var devices = new List<(string Name, string Serial)>();

        try
        {
            using var searcher = new ManagementObjectSearcher(@"SELECT * FROM Win32_PnPEntity WHERE PNPDeviceID LIKE 'USB%'");
            foreach (ManagementObject device in searcher.Get())
            {
                string pnpDeviceID = device["PNPDeviceID"]?.ToString() ?? "";
                if (string.IsNullOrEmpty(pnpDeviceID) || !pnpDeviceID.Contains("\\")) continue;

                string serial = pnpDeviceID.Split('\\').Last();
                if (!serial.Contains("&") && !serial.Contains(".") && !serial.Contains("{"))
                {
                    string name = device["Name"]?.ToString() ?? "";

                    devices.Add((name, serial));
                }
            }

            // Format devices with separators
            var deviceInfos = devices.Select(d => new[]
            {
                ("Device", d.Name),
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