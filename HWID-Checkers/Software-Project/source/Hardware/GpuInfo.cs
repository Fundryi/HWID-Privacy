using System.Diagnostics;
using System.Management;
using System.Text;

namespace HWIDChecker.Hardware;

public class GpuInfo : IHardwareInfo
{
    public string SectionTitle => "GPU INFO";

    public string GetInformation()
    {
        var sb = new StringBuilder();

        // Try NVIDIA-SMI first
        try
        {
            var process = new Process
            {
                StartInfo = new ProcessStartInfo
                {
                    FileName = "nvidia-smi",
                    Arguments = "-L",
                    UseShellExecute = false,
                    RedirectStandardOutput = true,
                    CreateNoWindow = true
                }
            };

            process.Start();
            string output = process.StandardOutput.ReadToEnd();
            process.WaitForExit();

            if (process.ExitCode == 0 && !string.IsNullOrWhiteSpace(output))
            {
                sb.Append(output.TrimEnd());
                return sb.ToString().TrimEnd();
            }
        }
        catch { }

        // Fallback to WMI
        using var searcher = new ManagementObjectSearcher("SELECT * FROM Win32_VideoController");
        foreach (ManagementObject gpu in searcher.Get())
        {
            sb.AppendLine($"Name: {gpu["Name"]}");
            sb.AppendLine($"PNPDeviceID: {gpu["PNPDeviceID"]}");
        }

        return sb.ToString();
    }
}