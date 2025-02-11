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
        public string WWN { get; set; } = "";
    }

    private string FormatAsTable(List<DiskInfo> disks)
    {
        if (!disks.Any()) return "No disk drives detected.";

        // Calculate column widths based on content
        var deviceIdWidth = Math.Max(10, disks.Max(d => d.DeviceId.Length));
        var driveLetterWidth = Math.Max(6, disks.Max(d => d.DriveLetter.Length));
        var volumeSerialWidth = Math.Max(12, disks.Max(d => d.VolumeSerial.Length));
        var serialWidth = Math.Max(12, disks.Max(d => d.SerialNumber.Length));
        var modelWidth = Math.Max(20, disks.Max(d => d.Model.Length));
        var firmwareWidth = Math.Max(9, disks.Max(d => d.FirmwareVersion.Length));
        var wwnWidth = Math.Max(5, disks.Max(d => d.WWN.Length));

        var sb = new StringBuilder();

        // Add headers
        sb.AppendLine();
        sb.AppendFormat($"{{0,-{deviceIdWidth}}} {{1,-{driveLetterWidth}}} {{2,-{volumeSerialWidth}}} {{3,-{serialWidth}}} {{4,-{modelWidth}}} {{5,-{firmwareWidth}}} {{6,-{wwnWidth}}}",
            "Device ID", "Drive", "Volume-SN", "Serial", "Model", "Firmware", "WWN");
        sb.AppendLine();

        // Add separator line
        sb.AppendLine(new string('-', deviceIdWidth + driveLetterWidth + volumeSerialWidth + serialWidth + modelWidth + firmwareWidth + wwnWidth + 6));

        // Add data rows
        foreach (var disk in disks)
        {
            sb.AppendFormat($"{{0,-{deviceIdWidth}}} {{1,-{driveLetterWidth}}} {{2,-{volumeSerialWidth}}} {{3,-{serialWidth}}} {{4,-{modelWidth}}} {{5,-{firmwareWidth}}} {{6,-{wwnWidth}}}",
                disk.DeviceId,
                disk.DriveLetter,
                disk.VolumeSerial,
                disk.SerialNumber,
                disk.Model,
                disk.FirmwareVersion,
                disk.WWN);
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
            var firmware = disk["FirmwareRevision"]?.ToString()?.Trim() ?? "";
            
            // Attempt to get WWN from Storage_Query_WWN property if available
            var wwn = "";
            try
            {
                var wwnValue = disk["WWN"]?.ToString();
                if (!string.IsNullOrEmpty(wwnValue))
                {
                    // Convert hex string to integer string if it's a valid hex value
                    if (long.TryParse(wwnValue, System.Globalization.NumberStyles.HexNumber, null, out long wwnInt))
                    {
                        wwn = wwnInt.ToString();
                    }
                    else
                    {
                        wwn = wwnValue;
                    }
                }
            }
            catch
            {
                // If WWN retrieval fails, leave it as empty string
            }

            // Find associated logical drive
            var logicalDrive = logicalDrives.FirstOrDefault(d => d.PhysicalDrive == deviceId);
            
            disks.Add(new DiskInfo
            {
                DeviceId = deviceId,
                DriveLetter = logicalDrive != default ? logicalDrive.DriveLetter.TrimEnd(':') : "",
                VolumeSerial = logicalDrive != default ? logicalDrive.VolumeSerial : "",
                Model = model,
                SerialNumber = serial,
                FirmwareVersion = firmware,
                WWN = wwn
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