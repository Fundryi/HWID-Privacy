using System.Management;
using System.Text;
using HWIDChecker.Services;

namespace HWIDChecker.Hardware;

public class MonitorInfo : IHardwareInfo
{
    private readonly TextFormattingService _textFormatter;
    public string SectionTitle => "MONITOR INFORMATION";

    public MonitorInfo(TextFormattingService textFormatter)
    {
        _textFormatter = textFormatter;
    }

    private string UInt16ArrayToString(ushort[] arr)
    {
        if (arr == null || arr.Length == 0) return string.Empty;
        var chars = arr.Where(u => u != 0).Select(u => (char)u);
        return new string(chars.ToArray());
    }

    public string GetInformation()
    {
        var sb = new StringBuilder();

        try
        {
            using var searcher = new ManagementObjectSearcher(@"root\wmi", "SELECT * FROM WmiMonitorID");
            var monitors = searcher.Get();

            if (monitors.Count == 0)
            {
                sb.AppendLine("No monitors detected. Please ensure your display drivers are properly installed.");
                return sb.ToString();
            }

            _textFormatter.AppendInfoLine(sb, "Count", $"{monitors.Count} monitor(s) found:");
            sb.AppendLine();

            var monitorInfos = new List<(string Label, string Value)[]>();

            foreach (ManagementObject monitor in monitors)
            {
                try
                {
                    var monitorDetails = new List<(string Label, string Value)>();

                    // Convert UInt16[] to string for each property
                    string manufacturer = UInt16ArrayToString((ushort[])monitor["ManufacturerName"]);
                    string model = UInt16ArrayToString((ushort[])monitor["UserFriendlyName"]);
                    string serial = UInt16ArrayToString((ushort[])monitor["SerialNumberID"]);
                    string productCode = UInt16ArrayToString((ushort[])monitor["ProductCodeID"]);

                    if (!string.IsNullOrEmpty(manufacturer))
                        monitorDetails.Add(("Manufacturer", manufacturer));
                    if (!string.IsNullOrEmpty(model))
                        monitorDetails.Add(("Model", model));
                    if (!string.IsNullOrEmpty(serial))
                        monitorDetails.Add(("Serial Number", serial));
                    if (!string.IsNullOrEmpty(productCode))
                        monitorDetails.Add(("Product Code", productCode));

                    var weekOfManufacture = monitor["WeekOfManufacture"];
                    var yearOfManufacture = monitor["YearOfManufacture"];
                    if (weekOfManufacture != null && yearOfManufacture != null)
                        monitorDetails.Add(("Manufacturing Date", $"Week {weekOfManufacture}, {yearOfManufacture}"));

                    monitorInfos.Add(monitorDetails.ToArray());
                }
                catch (Exception ex)
                {
                    monitorInfos.Add(new[] { ("Error", $"Error reading monitor details: {ex.Message}") });
                }
            }

            _textFormatter.AppendDeviceGroup(sb, monitorInfos);
        }
        catch (Exception ex)
        {
            sb.AppendLine($"Unable to retrieve monitor information. Error: {ex.Message}");
            sb.AppendLine("Please check if WMI service is running and you have sufficient permissions.");
        }

        return sb.ToString();
    }
}