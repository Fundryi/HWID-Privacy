using System.Diagnostics;
using System.Management;
using System.Text;
using HWIDChecker.Services;
using HWIDChecker.Services.Win32;

namespace HWIDChecker.Hardware;

public class DiskDriveInfo : IHardwareInfo
{
    private readonly TextFormattingService _textFormatter;

    public string SectionTitle => "DISK DRIVES";

    public DiskDriveInfo(TextFormattingService textFormatter)
    {
        _textFormatter = textFormatter;
    }

    private class DiskInfo
    {
        public string DeviceId { get; set; } = "";
        public string DriveLetter { get; set; } = "";
        public string VolumeSerial { get; set; } = "";
        public string Model { get; set; } = "";
        public string SerialNumber { get; set; } = "";
        public string FirmwareVersion { get; set; } = "";
        public string UniqueIdWmiHex { get; set; } = "";
        public string UniqueIdWmi { get; set; } = "";
        public string UniqueIdIoctlHex { get; set; } = "";
        public string UniqueIdIoctlDecoded { get; set; } = "";
        public bool WwnFromRawIoctl { get; set; } = false;
        public string PartitionStyle { get; set; } = "";
        public string DiskSignature { get; set; } = "";
        public List<string> PartitionGuids { get; set; } = new();
        public bool IsGptDisk => PartitionStyle == "GPT";
        public string HardwareId { get; set; } = "";
    }

    private string FormatAsTable(List<DiskInfo> disks)
    {
        if (!disks.Any()) return "No disk drives detected.";

        var sb = new StringBuilder();

        sb.AppendLine("Device ID");
        // Add separator line
        sb.AppendLine(new string('-', 50));

        // Add data rows
        for (int i = 0; i < disks.Count; i++)
        {
            var disk = disks[i];
            
            // Device ID line
            var deviceId = disk.DeviceId.Replace(@"\\.\", "");
            sb.AppendLine($"└── {deviceId}");
            
            // Drive and nested Volume-SN info with proper indentation
            sb.AppendLine($"    ├── Drive: {disk.DriveLetter}");
            sb.AppendLine($"    │   └── Volume-SN: {disk.VolumeSerial}");
            
            // Model, Serial, Hardware ID, and Firmware info
            sb.AppendLine($"    ├── Model: {disk.Model}");
            sb.AppendLine($"    ├── Serial: {disk.SerialNumber}");
            if (!string.IsNullOrEmpty(disk.HardwareId))
            {
                sb.AppendLine($"    ├── Hardware ID: {disk.HardwareId}");
            }

            var detailLines = new List<(string Label, string Value)>();
            if (!string.IsNullOrEmpty(disk.UniqueIdIoctlHex))
            {
                detailLines.Add(("UniqueId (IOCTL)", disk.UniqueIdIoctlHex));
                detailLines.Add(("UniqueId (IOCTL) decoded",
                    string.IsNullOrEmpty(disk.UniqueIdIoctlDecoded) ? "<empty>" : disk.UniqueIdIoctlDecoded));
            }
            if (!string.IsNullOrEmpty(disk.UniqueIdWmiHex))
            {
                detailLines.Add(("UniqueId (WMI)", disk.UniqueIdWmiHex));
                detailLines.Add(("UniqueId (WMI) decoded",
                    string.IsNullOrEmpty(disk.UniqueIdWmi) ? "<empty>" : disk.UniqueIdWmi));
            }
            if (!string.IsNullOrEmpty(disk.PartitionStyle))
            {
                var label = disk.IsGptDisk
                    ? $"Partition Style: {disk.PartitionStyle} | Disk GUID"
                    : $"Partition Style: {disk.PartitionStyle} | Disk Signature";
                detailLines.Add((label, disk.DiskSignature));

                foreach (var partGuid in disk.PartitionGuids)
                {
                    detailLines.Add(("  Partition GUID", partGuid));
                }
            }

            // Firmware info (end of tree if no detail data)
            if (detailLines.Count == 0)
            {
                sb.AppendLine($"    └── Firmware: {disk.FirmwareVersion}");
            }
            else
            {
                sb.AppendLine($"    ├── Firmware: {disk.FirmwareVersion}");
                for (int lineIndex = 0; lineIndex < detailLines.Count; lineIndex++)
                {
                    var prefix = lineIndex == detailLines.Count - 1 ? "    └──" : "    ├──";
                    var line = detailLines[lineIndex];
                    sb.AppendLine($"{prefix} {line.Label}: {line.Value}");
                }
            }
            
            // Add separator between drives, but not after the last one
            if (i < disks.Count - 1)
            {
                sb.AppendLine(new string('-', 50));
            }
        }

        return sb.ToString();
    }

    public string GetInformation()
    {
        var disks = new List<DiskInfo>();
        var logicalDrives = GetLogicalDrives();
        var physicalDiskUniqueIdMap = GetPhysicalDiskUniqueIdMap();
        Dictionary<string, string> hwIdMap = null;
        try { hwIdMap = SetupApi.GetHardwareIdMap(); } catch { }

        using var searcher = new ManagementObjectSearcher("SELECT * FROM Win32_DiskDrive");
        foreach (ManagementObject disk in searcher.Get())
        {
            var deviceId = disk["DeviceID"]?.ToString() ?? "Unknown Device";
            var model = disk["Model"]?.ToString()?.Trim() ?? "Unknown Model";
            var serial = disk["SerialNumber"]?.ToString()?.Trim() ?? "Unknown Serial";
            var firmware = disk["FirmwareRevision"]?.ToString()?.Trim() ?? "";
            
            // Get disk index for WWN query
            int diskIndex = -1;
            if (int.TryParse(disk["Index"]?.ToString(), out int parsedIndex))
            {
                diskIndex = parsedIndex;
            }

            // Find associated logical drive
            var logicalDrive = logicalDrives.FirstOrDefault(d => d.PhysicalDrive == deviceId);
            
            var pnpDeviceId = disk["PNPDeviceID"]?.ToString() ?? "";

            var diskInfo = new DiskInfo
            {
                DeviceId = deviceId,
                DriveLetter = logicalDrive != default ? logicalDrive.DriveLetter.TrimEnd(':') : "",
                VolumeSerial = logicalDrive != default ? logicalDrive.VolumeSerial : "",
                Model = model,
                SerialNumber = serial,
                FirmwareVersion = firmware
            };

            // SetupAPI Hardware ID
            if (hwIdMap != null && !string.IsNullOrEmpty(pnpDeviceId) &&
                hwIdMap.TryGetValue(pnpDeviceId, out var hwId))
            {
                diskInfo.HardwareId = hwId;
            }

            if (diskIndex >= 0)
            {
                if (StorageDeviceIdQuery.TryGetWwnHexFromPhysicalDrive(diskIndex, out var wwnHex, out var decoded))
                {
                    diskInfo.UniqueIdIoctlHex = wwnHex;
                    diskInfo.UniqueIdIoctlDecoded = decoded;
                    diskInfo.WwnFromRawIoctl = true;
                }

                var layout = StorageDeviceIdQuery.TryGetDiskLayout(diskIndex);
                if (layout != null)
                {
                    diskInfo.PartitionStyle = layout.IsGpt ? "GPT" : "MBR";
                    diskInfo.DiskSignature = layout.IsGpt
                        ? layout.DiskGuid?.ToString("D").ToUpper() ?? ""
                        : $"0x{layout.MbrSignature:X8}";
                    if (layout.IsGpt && layout.PartitionGuids.Count > 0)
                    {
                        diskInfo.PartitionGuids = layout.PartitionGuids
                            .Select(g => g.ToString("D").ToUpper())
                            .ToList();
                    }
                }
            }

            // Try to get UniqueId information - use WMI/PowerShell directly (existing fallback)
            if (diskIndex >= 0)
            {
                if (physicalDiskUniqueIdMap.TryGetValue(diskIndex, out var uniqueIdInfo))
                {
                    diskInfo.UniqueIdWmiHex = uniqueIdInfo.UniqueIdHex;
                    diskInfo.UniqueIdWmi = uniqueIdInfo.UniqueId;
                }
            }
            
            disks.Add(diskInfo);
        }

        return FormatAsTable(disks);
    }

    private Dictionary<int, (string UniqueId, string UniqueIdHex)> GetPhysicalDiskUniqueIdMap()
    {
        var result = new Dictionary<int, (string, string)>();
        
        try
        {
            // Query MSFT_PhysicalDisk for UniqueId
            using var searcher = new ManagementObjectSearcher("SELECT DeviceId, UniqueId FROM MSFT_PhysicalDisk");
            foreach (ManagementObject disk in searcher.Get())
            {
                var deviceId = disk["DeviceId"]?.ToString();
                var uniqueId = disk["UniqueId"]?.ToString();
                
                if (!string.IsNullOrEmpty(deviceId) && int.TryParse(deviceId, out int diskIndex) && !string.IsNullOrEmpty(uniqueId))
                {
                    // Convert UniqueId to hex
                    string uniqueIdHex = ConvertUniqueIdToHex(uniqueId);
                    result[diskIndex] = (uniqueId, uniqueIdHex);
                }
            }
        }
        catch
        {
            // MSFT_PhysicalDisk may not be available on all systems - try PowerShell as fallback
            result = GetPhysicalDiskUniqueIdFromPowerShell();
        }
        
        return result;
    }

    private Dictionary<int, (string UniqueId, string UniqueIdHex)> GetPhysicalDiskUniqueIdFromPowerShell()
    {
        var result = new Dictionary<int, (string, string)>();
        
        try
        {
            // Use PowerShell to Get-PhysicalDisk
            var psi = new ProcessStartInfo
            {
                FileName = "powershell.exe",
                Arguments = "-NoProfile -Command \"Get-PhysicalDisk | Select-Object DeviceId, UniqueId | ConvertTo-Json\"",
                RedirectStandardOutput = true,
                UseShellExecute = false,
                CreateNoWindow = true
            };

            using var process = Process.Start(psi);
            if (process != null)
            {
                string output = process.StandardOutput.ReadToEnd();
                process.WaitForExit();

                // Parse JSON output
                if (!string.IsNullOrEmpty(output) && output != "[]")
                {
                    var disks = System.Text.Json.JsonSerializer.Deserialize<System.Text.Json.JsonElement>(output);
                    if (disks.ValueKind == System.Text.Json.JsonValueKind.Array)
                    {
                        foreach (var disk in disks.EnumerateArray())
                        {
                            if (disk.TryGetProperty("DeviceId", out var deviceIdProp) &&
                                disk.TryGetProperty("UniqueId", out var uniqueIdProp))
                            {
                                string deviceId = deviceIdProp.ToString();
                                string uniqueId = uniqueIdProp.ToString();

                                if (!string.IsNullOrEmpty(deviceId) && int.TryParse(deviceId, out int diskIndex) && !string.IsNullOrEmpty(uniqueId))
                                {
                                    string uniqueIdHex = ConvertUniqueIdToHex(uniqueId);
                                    result[diskIndex] = (uniqueId, uniqueIdHex);
                                }
                            }
                        }
                    }
                }
            }
        }
        catch
        {
            // PowerShell method failed
        }
        
        return result;
    }

    private string ConvertUniqueIdToHex(string uniqueId)
    {
        if (string.IsNullOrEmpty(uniqueId))
        {
            return string.Empty;
        }

        // Try to parse as GUID
        if (Guid.TryParse(uniqueId, out Guid guid))
        {
            byte[] bytes = guid.ToByteArray();
            var sb = new StringBuilder(bytes.Length * 3);
            for (int i = 0; i < bytes.Length; i++)
            {
                if (i > 0)
                {
                    sb.Append(':');
                }
                sb.Append(bytes[i].ToString("X2"));
            }
            return sb.ToString();
        }

        // If it's already ASCII, convert to hex
        var hexSb = new StringBuilder(uniqueId.Length * 3);
        foreach (char c in uniqueId)
        {
            if (hexSb.Length > 0)
            {
                hexSb.Append(':');
            }
            hexSb.Append(((byte)c).ToString("X2"));
        }
        return hexSb.ToString();
    }

    private List<(string PhysicalDrive, string DriveLetter, string VolumeSerial)> GetLogicalDrives()
    {
        var result = new List<(string PhysicalDrive, string DriveLetter, string VolumeSerial)>();
        using var logicalDiskSearcher = new ManagementObjectSearcher("SELECT * FROM Win32_LogicalDisk");

        foreach (ManagementObject disk in logicalDiskSearcher.Get())
        {
            var driveLetter = disk["DeviceID"]?.ToString() ?? "Unknown";
            var volumeSerial = disk["VolumeSerialNumber"]?.ToString() ?? "";

            // Use DiskDriveToDiskPartition and LogicalDiskToPartition to find physical drive
            using var partitionSearcher = new ManagementObjectSearcher(
                $"ASSOCIATORS OF {{Win32_LogicalDisk.DeviceID='{driveLetter}'}} WHERE AssocClass = Win32_LogicalDiskToPartition");

            foreach (var partition in partitionSearcher.Get())
            {
                using var physicalDriveSearcher = new ManagementObjectSearcher(
                    $"ASSOCIATORS OF {{Win32_DiskPartition.DeviceID='{partition["DeviceID"]}'}} WHERE AssocClass = Win32_DiskDriveToDiskPartition");

                foreach (var drive in physicalDriveSearcher.Get())
                {
                    var physicalDeviceId = drive["DeviceID"]?.ToString() ?? "Unknown";
                    result.Add((physicalDeviceId, driveLetter, volumeSerial));
                }
            }
        }

        return result;
    }
}
