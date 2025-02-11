using System.Management;
using System.Text;
using HWIDChecker.Services;

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
            
            // Model, Serial, and Firmware info
            sb.AppendLine($"    ├── Model: {disk.Model}");
            sb.AppendLine($"    ├── Serial: {disk.SerialNumber}");
            sb.AppendLine($"    └── Firmware: {disk.FirmwareVersion}");
            
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

        using var searcher = new ManagementObjectSearcher("SELECT * FROM Win32_DiskDrive");
        foreach (ManagementObject disk in searcher.Get())
        {
            var deviceId = disk["DeviceID"]?.ToString() ?? "Unknown Device";
            var model = disk["Model"]?.ToString()?.Trim() ?? "Unknown Model";
            var serial = disk["SerialNumber"]?.ToString()?.Trim() ?? "Unknown Serial";
            var firmware = disk["FirmwareRevision"]?.ToString()?.Trim() ?? "";

            // Find associated logical drive
            var logicalDrive = logicalDrives.FirstOrDefault(d => d.PhysicalDrive == deviceId);
            
            disks.Add(new DiskInfo
            {
                DeviceId = deviceId,
                DriveLetter = logicalDrive != default ? logicalDrive.DriveLetter.TrimEnd(':') : "",
                VolumeSerial = logicalDrive != default ? logicalDrive.VolumeSerial : "",
                Model = model,
                SerialNumber = serial,
                FirmwareVersion = firmware
            });
        }

        return FormatAsTable(disks);
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