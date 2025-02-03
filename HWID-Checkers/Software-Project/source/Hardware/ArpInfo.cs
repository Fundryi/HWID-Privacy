using System.Diagnostics;
using System.Text;
using HWIDChecker.Services;

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

            // Filter out common Windows entries and format the output
            var lines = output.Split(new[] { '\r', '\n' }, StringSplitOptions.RemoveEmptyEntries);
            bool isHeaderPrinted = false;

            foreach (var line in lines)
            {
                // Skip interface headers and empty lines
                if (line.Contains("Interface:"))
                {
                    if (!isHeaderPrinted)
                    {
                        sb.AppendLine("Dynamic ARP Entries:");
                        isHeaderPrinted = true;
                    }
                    continue;
                }

                // Skip lines that don't contain MAC addresses
                if (!line.Contains("-"))
                    continue;

                // Skip common Windows broadcast/multicast addresses
                if (line.Contains("ff-ff-ff-ff-ff-ff") ||
                    line.Contains("01-00-5e") ||
                    line.EndsWith("static"))
                    continue;

                // Only include dynamic entries as they are more relevant
                if (line.Contains("dynamic"))
                {
                    var parts = line.Split(new[] { ' ' }, StringSplitOptions.RemoveEmptyEntries);
                    if (parts.Length >= 3)
                    {
                        _textFormatter.AppendCombinedInfoLine(sb,
                            ("MAC", parts[1].Replace('-', ':')),
                            ("IP", parts[0]));
                    }
                }
            }

            if (sb.Length == 0)
            {
                _textFormatter.AppendInfoLine(sb, "Status", "No relevant dynamic ARP entries found.");
            }
        }
        catch (Exception ex)
        {
            _textFormatter.AppendInfoLine(sb, "Error", $"Unable to retrieve ARP information: {ex.Message}");
        }

        return sb.ToString().TrimEnd();
    }
}