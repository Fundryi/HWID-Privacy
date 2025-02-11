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
                string[] lines = output.Split('\n', StringSplitOptions.RemoveEmptyEntries);
                foreach (string line in lines)
                {
                    if (line.StartsWith("GPU "))
                    {
                        // Extract GPU index
                        var parts = line.Split(':')[0].Trim();
                        sb.AppendLine(parts);

                        // Extract GPU name and UUID
                        var info = line.Split(':', 2)[1].Trim();
                        var gpuParts = info.Split("(UUID", 2);
                        
                        // Add GPU name
                        sb.AppendLine($"└── {gpuParts[0].Trim()}");
                        
                        // Add UUID if present
                        if (gpuParts.Length > 1)
                        {
                            sb.AppendLine($"    └── UUID{gpuParts[1].TrimEnd(')')}");
                        }
                    }
                }
                return sb.ToString().TrimEnd();
            }
        }
        catch { }

        // Fallback to WMI
        using var searcher = new ManagementObjectSearcher("SELECT * FROM Win32_VideoController");
        foreach (ManagementObject gpu in searcher.Get())
        {
            var name = gpu["Name"]?.ToString() ?? "Unknown";
            var pnpId = gpu["PNPDeviceID"]?.ToString() ?? "Unknown";
            sb.AppendLine($"└── {name}");
            sb.AppendLine($"    └── {pnpId}");
        }

        return sb.ToString();
    }
}