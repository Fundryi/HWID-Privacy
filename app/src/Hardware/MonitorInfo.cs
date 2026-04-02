using System.Management;
using System.Text;
using HWIDChecker.Services;
using Microsoft.Win32;

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
                // WMI failed (headless, RDP, no display driver) — try registry EDID fallback
                var registryMonitors = GetMonitorsFromRegistryEdid();
                if (registryMonitors.Count > 0)
                {
                    _textFormatter.AppendInfoLine(sb, "Count", $"{registryMonitors.Count} monitor(s) found (from registry):");
                    sb.AppendLine();
                    _textFormatter.AppendDeviceGroup(sb, registryMonitors);
                    return sb.ToString();
                }

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

                    // Try to enrich with EDID numeric serial from registry
                    var edidSerial = TryGetEdidNumericSerial(manufacturer, model);
                    if (edidSerial.HasValue && edidSerial.Value != 0)
                        monitorDetails.Add(("EDID Serial (numeric)", $"0x{edidSerial.Value:X8}"));

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

    /// <summary>
    /// Reads EDID numeric serial (bytes 12-15 of base EDID block) from registry.
    /// Tries to match by manufacturer/model to the right registry entry.
    /// </summary>
    private static uint? TryGetEdidNumericSerial(string manufacturer, string model)
    {
        try
        {
            using var displayKey = Registry.LocalMachine.OpenSubKey(
                @"SYSTEM\CurrentControlSet\Enum\DISPLAY");
            if (displayKey == null) return null;

            foreach (var monitorId in displayKey.GetSubKeyNames())
            {
                using var monitorKey = displayKey.OpenSubKey(monitorId);
                if (monitorKey == null) continue;

                foreach (var instanceId in monitorKey.GetSubKeyNames())
                {
                    using var instanceKey = monitorKey.OpenSubKey(instanceId);
                    using var paramsKey = instanceKey?.OpenSubKey("Device Parameters");
                    if (paramsKey == null) continue;

                    if (paramsKey.GetValue("EDID") is not byte[] edid || edid.Length < 16)
                        continue;

                    // EDID bytes 12-15 are the numeric serial number (little-endian uint32)
                    uint serial = BitConverter.ToUInt32(edid, 12);

                    // Try to match this EDID to the right monitor
                    // EDID bytes 8-9 are manufacturer ID (3-letter PNP ID encoded)
                    string edidMfg = DecodeEdidManufacturer(edid);
                    if (!string.IsNullOrEmpty(manufacturer) && !string.IsNullOrEmpty(edidMfg) &&
                        manufacturer.Contains(edidMfg, StringComparison.OrdinalIgnoreCase))
                    {
                        return serial;
                    }

                    // If we can't match by manufacturer, return first non-zero serial found
                    if (string.IsNullOrEmpty(manufacturer) && serial != 0)
                        return serial;
                }
            }
        }
        catch { }

        return null;
    }

    /// <summary>
    /// Decodes the 3-letter PNP manufacturer ID from EDID bytes 8-9.
    /// </summary>
    private static string DecodeEdidManufacturer(byte[] edid)
    {
        if (edid.Length < 10) return null;

        // EDID bytes 8-9: manufacturer ID encoded as 3 5-bit chars (A=1, B=2, ...)
        ushort raw = (ushort)((edid[8] << 8) | edid[9]);
        char c1 = (char)(((raw >> 10) & 0x1F) + 'A' - 1);
        char c2 = (char)(((raw >> 5) & 0x1F) + 'A' - 1);
        char c3 = (char)((raw & 0x1F) + 'A' - 1);

        return $"{c1}{c2}{c3}";
    }

    /// <summary>
    /// Fallback: reads all EDID blobs from registry when WMI WmiMonitorID returns nothing.
    /// </summary>
    private List<(string Label, string Value)[]> GetMonitorsFromRegistryEdid()
    {
        var result = new List<(string Label, string Value)[]>();

        try
        {
            using var displayKey = Registry.LocalMachine.OpenSubKey(
                @"SYSTEM\CurrentControlSet\Enum\DISPLAY");
            if (displayKey == null) return result;

            foreach (var monitorId in displayKey.GetSubKeyNames())
            {
                using var monitorKey = displayKey.OpenSubKey(monitorId);
                if (monitorKey == null) continue;

                foreach (var instanceId in monitorKey.GetSubKeyNames())
                {
                    using var instanceKey = monitorKey.OpenSubKey(instanceId);
                    using var paramsKey = instanceKey?.OpenSubKey("Device Parameters");
                    if (paramsKey == null) continue;

                    if (paramsKey.GetValue("EDID") is not byte[] edid || edid.Length < 128)
                        continue;

                    var details = new List<(string Label, string Value)>();

                    string mfg = DecodeEdidManufacturer(edid);
                    if (!string.IsNullOrEmpty(mfg))
                        details.Add(("Manufacturer", mfg));

                    // Parse descriptor blocks (bytes 54-125) for model name and string serial
                    for (int offset = 54; offset <= 108; offset += 18)
                    {
                        if (edid[offset] != 0 || edid[offset + 1] != 0) continue; // Not a monitor descriptor
                        byte tag = edid[offset + 3];

                        // Tag 0xFC = monitor name, 0xFF = serial string
                        if (tag == 0xFC || tag == 0xFF)
                        {
                            string text = System.Text.Encoding.ASCII
                                .GetString(edid, offset + 5, 13)
                                .TrimEnd('\n', '\r', ' ', '\0');
                            if (!string.IsNullOrEmpty(text))
                            {
                                details.Add((tag == 0xFC ? "Model" : "Serial Number", text));
                            }
                        }
                    }

                    uint numericSerial = BitConverter.ToUInt32(edid, 12);
                    if (numericSerial != 0)
                        details.Add(("EDID Serial (numeric)", $"0x{numericSerial:X8}"));

                    if (details.Count > 0)
                        result.Add(details.ToArray());
                }
            }
        }
        catch { }

        return result;
    }
}