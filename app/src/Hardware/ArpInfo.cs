using System.Diagnostics;
using System.Net.NetworkInformation;
using System.Text;
using HWIDChecker.Services;
using HWIDChecker.Services.Win32;

namespace HWIDChecker.Hardware;

public class ArpInfo : IHardwareInfo
{
    private readonly TextFormattingService _textFormatter;

    public string SectionTitle => "ARP INFO/CACHE";

    public ArpInfo(TextFormattingService textFormatter)
    {
        _textFormatter = textFormatter;
    }

    public string GetInformation()
    {
        var sb = new StringBuilder();

        // Try direct P/Invoke first (locale-independent, supports IPv6)
        var entries = IpHlpApi.GetNeighborTable();
        if (entries != null && entries.Count > 0)
        {
            FormatNeighborEntries(sb, entries);
            return sb.ToString().TrimEnd();
        }

        // Fallback to arp.exe (locale-dependent, IPv4 only)
        if (entries == null)
        {
            FormatArpExeFallback(sb);
            return sb.ToString().TrimEnd();
        }

        _textFormatter.AppendInfoLine(sb, "Status", "No relevant dynamic ARP entries found.");
        return sb.ToString().TrimEnd();
    }

    private void FormatNeighborEntries(StringBuilder sb, List<IpHlpApi.NeighborEntry> entries)
    {
        // Resolve interface index to friendly name
        var interfaceNames = GetInterfaceNames();

        // Group by interface
        var grouped = entries
            .GroupBy(e => e.InterfaceIndex)
            .OrderBy(g => g.Key);

        foreach (var group in grouped)
        {
            string ifName = interfaceNames.TryGetValue(group.Key, out var name)
                ? name
                : $"Interface #{group.Key}";

            sb.AppendLine($"[{ifName}]");

            // IPv4 entries first, then IPv6
            var ipv4 = group.Where(e => !e.IsIpv6).OrderBy(e => e.IpAddress);
            var ipv6 = group.Where(e => e.IsIpv6).OrderBy(e => e.IpAddress);

            foreach (var entry in ipv4)
            {
                _textFormatter.AppendCombinedInfoLine(sb,
                    ("MAC", entry.PhysicalAddress),
                    ("IP", entry.IpAddress));
            }

            foreach (var entry in ipv6)
            {
                _textFormatter.AppendCombinedInfoLine(sb,
                    ("MAC", entry.PhysicalAddress),
                    ("IPv6", entry.IpAddress));
            }

            sb.AppendLine();
        }
    }

    private static Dictionary<uint, string> GetInterfaceNames()
    {
        var result = new Dictionary<uint, string>();
        try
        {
            foreach (var nic in NetworkInterface.GetAllNetworkInterfaces())
            {
                var props = nic.GetIPProperties();
                var ipv4Props = props.GetIPv4Properties();
                if (ipv4Props != null)
                {
                    result[(uint)ipv4Props.Index] = nic.Name;
                }
            }
        }
        catch { }
        return result;
    }

    private void FormatArpExeFallback(StringBuilder sb)
    {
        try
        {
            var process = new Process
            {
                StartInfo = new ProcessStartInfo
                {
                    FileName = "arp",
                    Arguments = "-a",
                    UseShellExecute = false,
                    RedirectStandardOutput = true,
                    CreateNoWindow = true
                }
            };

            process.Start();
            string output = process.StandardOutput.ReadToEnd();
            process.WaitForExit();

            var lines = output.Split(new[] { '\r', '\n' }, StringSplitOptions.RemoveEmptyEntries);
            bool hasEntries = false;

            foreach (var line in lines)
            {
                if (line.Contains("Interface:") || !line.Contains("-"))
                    continue;

                if (line.Contains("ff-ff-ff-ff-ff-ff") ||
                    line.Contains("01-00-5e") ||
                    line.EndsWith("static"))
                    continue;

                if (line.Contains("dynamic"))
                {
                    var parts = line.Split(new[] { ' ' }, StringSplitOptions.RemoveEmptyEntries);
                    if (parts.Length >= 3)
                    {
                        if (!hasEntries)
                        {
                            sb.AppendLine("Dynamic ARP Entries:");
                            hasEntries = true;
                        }
                        _textFormatter.AppendCombinedInfoLine(sb,
                            ("MAC", parts[1].Replace('-', ':')),
                            ("IP", parts[0]));
                    }
                }
            }

            if (!hasEntries)
            {
                _textFormatter.AppendInfoLine(sb, "Status", "No relevant dynamic ARP entries found.");
            }
        }
        catch (Exception ex)
        {
            _textFormatter.AppendInfoLine(sb, "Error", $"Unable to retrieve ARP information: {ex.Message}");
        }
    }
}
