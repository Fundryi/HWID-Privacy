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
    }

    private string FormatAsTable(List<DiskInfo> disks)
    {
        if (!disks.Any()) return "No disk drives detected.";

        // Calculate column widths based on content
        var deviceIdWidth = Math.Max(10, disks.Max(d => d.DeviceId.Length));
        var driveLetterWidth = Math.Max(6, disks.Max(d => d.DriveLetter.Length));
        var volumeSerialWidth = Math.Max(12, disks.Max(d => d.VolumeSerial.Length));
        var modelWidth = Math.Max(20, disks.Max(d => d.Model.Length));
        var serialWidth = Math.Max(12, disks.Max(d => d.SerialNumber.Length));

        var sb = new StringBuilder();

        // Add headers
        sb.AppendLine();
        sb.AppendFormat($"{{0,-{deviceIdWidth}}} {{1,-{driveLetterWidth}}} {{2,-{volumeSerialWidth}}} {{3,-{modelWidth}}} {{4,-{serialWidth}}}",
            "Device ID", "Drive", "Volume-SN", "Model", "Serial");
        sb.AppendLine();

        // Add separator line
        sb.AppendLine(new string('-', deviceIdWidth + driveLetterWidth + volumeSerialWidth + modelWidth + serialWidth + 4));

        // Add data rows
        foreach (var disk in disks)
        {
            sb.AppendFormat($"{{0,-{deviceIdWidth}}} {{1,-{driveLetterWidth}}} {{2,-{volumeSerialWidth}}} {{3,-{modelWidth}}} {{4,-{serialWidth}}}",
                disk.DeviceId,
                disk.DriveLetter,
                disk.VolumeSerial,
                disk.Model,
                disk.SerialNumber);
            sb.AppendLine();
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

            // Find associated logical drive
            var logicalDrive = logicalDrives.FirstOrDefault(d => d.PhysicalDrive == deviceId);
            
            disks.Add(new DiskInfo
            {
                DeviceId = deviceId,
                DriveLetter = logicalDrive != default ? logicalDrive.DriveLetter.TrimEnd(':') : "",
                VolumeSerial = logicalDrive != default ? logicalDrive.VolumeSerial : "",
                Model = model,
                SerialNumber = serial
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