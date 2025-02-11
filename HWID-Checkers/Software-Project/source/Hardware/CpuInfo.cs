using System.Management;
using System.Text;

namespace HWIDChecker.Hardware;

public class CpuInfo : IHardwareInfo
{
    public string SectionTitle => "CPU";

    public string GetInformation()
    {
        var sb = new StringBuilder();
        using var searcher = new ManagementObjectSearcher("SELECT * FROM Win32_Processor");
        foreach (ManagementObject cpu in searcher.Get())
        {
            sb.AppendLine($"Name: {cpu["Name"]}");
            sb.AppendLine($"ProcessorId: {cpu["ProcessorId"]}");
            if (cpu["SerialNumber"] != null)
            {
                sb.AppendLine($"SerialNumber: {cpu["SerialNumber"]}");
            }
        }
        return sb.ToString();
    }
}