using System.Diagnostics;
using System.Management;
using System.Text;
using HWIDChecker.Services;

namespace HWIDChecker.Hardware;

public class GpuInfo : IHardwareInfo
{
    private readonly TextFormattingService textFormatter;

    public string SectionTitle => "GPU INFO";

    public GpuInfo(TextFormattingService textFormatter = null)
    {
        this.textFormatter = textFormatter;
    }

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
        var gpus = searcher.Get().Cast<ManagementObject>().ToList();

        if (!gpus.Any())
        {
            return "No GPU detected.";
        }

        for (int i = 0; i < gpus.Count; i++)
        {
            var gpu = gpus[i];
            var name = gpu["Name"]?.ToString() ?? "Unknown";
            
            sb.AppendLine($"GPU {i}");
            sb.AppendLine($"└── {name}");
            sb.AppendLine($"    └── {gpu["PNPDeviceID"]?.ToString() ?? "Unknown"}");

            // Add a line break between GPUs except for the last one
            if (i < gpus.Count - 1)
            {
                sb.AppendLine();
            }
        }

        return sb.ToString().TrimEnd();
    }
}