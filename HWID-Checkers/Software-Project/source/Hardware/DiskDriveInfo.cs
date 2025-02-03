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

    public string GetInformation()
    {
        var sb = new StringBuilder();
        var logicalDrives = GetLogicalDrives();

        using var searcher = new ManagementObjectSearcher("SELECT * FROM Win32_DiskDrive");
        foreach (ManagementObject disk in searcher.Get())
        {
            var deviceId = disk["DeviceID"].ToString();
            var model = disk["Model"].ToString().Trim();
            var serial = disk["SerialNumber"].ToString().Trim();

            // Find associated logical drive
            var logicalDrive = logicalDrives.FirstOrDefault(d => d.PhysicalDrive == deviceId);
            if (logicalDrive != default)
            {
                _textFormatter.AppendCombinedInfoLine(sb,
                    (deviceId, ""),
                    ($"{logicalDrive.DriveLetter}", logicalDrive.VolumeSerial),
                    (model, serial));
            }
            else
            {
                _textFormatter.AppendCombinedInfoLine(sb,
                    (deviceId, ""),
                    ("", ""),
                    (model, serial));
            }
        }

        return sb.ToString();
    }

    private List<(string PhysicalDrive, string DriveLetter, string VolumeSerial)> GetLogicalDrives()
    {
        var result = new List<(string PhysicalDrive, string DriveLetter, string VolumeSerial)>();
        using var logicalDiskSearcher = new ManagementObjectSearcher("SELECT * FROM Win32_LogicalDisk");

        foreach (ManagementObject disk in logicalDiskSearcher.Get())
        {
            var driveLetter = disk["DeviceID"].ToString();
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
                    result.Add((drive["DeviceID"].ToString(), driveLetter, volumeSerial));
                }
            }
        }

        return result;
    }
}