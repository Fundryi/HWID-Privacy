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

    private class RamModuleInfo
    {
        public string DeviceLocator { get; set; } = "";
        public string Manufacturer { get; set; } = "";
        public string PartNumber { get; set; } = "";
        public string Capacity { get; set; } = "";
        public string SerialNumber { get; set; } = "";
    }

    private string FormatAsTable(List<RamModuleInfo> modules)
    {
        if (!modules.Any()) return "No RAM modules detected.";

        // Calculate column widths based on content
        var deviceLocatorWidth = Math.Max(15, modules.Max(m => m.DeviceLocator.Length));
        var manufacturerWidth = Math.Max(12, modules.Max(m => m.Manufacturer.Length));
        var partNumberWidth = Math.Max(10, modules.Max(m => m.PartNumber.Length));
        var capacityWidth = Math.Max(8, modules.Max(m => m.Capacity.Length));
        var serialNumberWidth = Math.Max(12, modules.Max(m => m.SerialNumber.Length));

        var sb = new StringBuilder();

        // Add headers
        sb.AppendFormat($"{{0,-{deviceLocatorWidth}}} {{1,-{manufacturerWidth}}} {{2,-{partNumberWidth}}} {{3,-{capacityWidth}}} {{4,-{serialNumberWidth}}}",
            "DeviceLocator", "Manufacturer", "PartNumber", "Capacity", "SerialNumber");
        sb.AppendLine();

        // Add separator line
        sb.AppendLine(new string('-', deviceLocatorWidth + manufacturerWidth + partNumberWidth + capacityWidth + serialNumberWidth + 4));

        // Add data rows
        foreach (var module in modules)
        {
            sb.AppendFormat($"{{0,-{deviceLocatorWidth}}} {{1,-{manufacturerWidth}}} {{2,-{partNumberWidth}}} {{3,-{capacityWidth}}} {{4,-{serialNumberWidth}}}",
                module.DeviceLocator,
                module.Manufacturer,
                module.PartNumber,
                module.Capacity,
                module.SerialNumber);
            sb.AppendLine();
        }

        return sb.ToString();
    }

    public string GetInformation()
    {
        var modules = new List<RamModuleInfo>();
        using var searcher = new ManagementObjectSearcher("SELECT * FROM Win32_PhysicalMemory");

        foreach (ManagementObject ram in searcher.Get())
        {
            var capacityBytes = Convert.ToUInt64(ram["Capacity"] ?? 0);
            var capacityGB = capacityBytes / (1024.0 * 1024.0 * 1024.0);

            modules.Add(new RamModuleInfo
            {
                DeviceLocator = ram["DeviceLocator"]?.ToString() ?? "",
                Manufacturer = ram["Manufacturer"]?.ToString() ?? "",
                PartNumber = ram["PartNumber"]?.ToString() ?? "",
                Capacity = $"{capacityGB:N0} GB",
                SerialNumber = ram["SerialNumber"]?.ToString() ?? ""
            });
        }

        return FormatAsTable(modules);
    }
}