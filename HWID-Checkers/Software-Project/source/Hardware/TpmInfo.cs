using System.Diagnostics;
using System.Text;
using HWIDChecker.Services;

namespace HWIDChecker.Hardware;

public class TpmInfo : IHardwareInfo
{
    private readonly TextFormattingService textFormatter;

    public string SectionTitle => "TPM MODULES";

    public TpmInfo(TextFormattingService textFormatter = null)
    {
        this.textFormatter = textFormatter;
    }

    public string GetInformation()
    {
        var sb = new StringBuilder();

        try
        {
            // First check if TPM is present and enabled
            var tpmProcess = new Process
            {
                StartInfo = new ProcessStartInfo
                {
                    FileName = "powershell",
                    Arguments = "-Command \"Get-Tpm\"",
                    UseShellExecute = false,
                    RedirectStandardOutput = true,
                    CreateNoWindow = true
                }
            };

            tpmProcess.Start();
            string tpmOutput = tpmProcess.StandardOutput.ReadToEnd();
            tpmProcess.WaitForExit();

            // If TPM is not present at all, return TPM OFF
            if (string.IsNullOrWhiteSpace(tpmOutput) || !tpmOutput.Contains("TpmPresent") ||
                (tpmOutput.Contains("TpmPresent") && !tpmOutput.Contains("True")))
            {
                sb.AppendLine("TPM OFF");
                return sb.ToString();
            }

            // Always show TPM state first (enabled or disabled) before showing details
            var tpmLines = tpmOutput.Split('\n');
            bool isEnabled = false;
            foreach (var line in tpmLines)
            {
                if (line.Contains("TpmEnabled") && line.Contains("True"))
                {
                    isEnabled = true;
                    break;
                }
            }
            
            sb.AppendLine(isEnabled ? "TPM: ENABLED" : "TPM: DISABLED");

            // Get detailed TPM information
            var process = new Process
            {
                StartInfo = new ProcessStartInfo
                {
                    FileName = "powershell",
                    Arguments = "-Command \"Get-TpmEndorsementKeyInfo -Hash 'Sha256' | Format-List\"",
                    UseShellExecute = false,
                    RedirectStandardOutput = true,
                    CreateNoWindow = true
                }
            };

            process.Start();
            string output = process.StandardOutput.ReadToEnd();
            process.WaitForExit();

            if (string.IsNullOrWhiteSpace(output))
            {
                sb.AppendLine("Unable to retrieve detailed TPM information");
                return sb.ToString();
            }

            // Process and format the output
            var lines = output.Split(new[] { '\r', '\n' }, StringSplitOptions.RemoveEmptyEntries);
            var processedLines = new Dictionary<string, string>();

            foreach (var line in lines)
            {
                var trimmedLine = line.Trim();
                if (string.IsNullOrWhiteSpace(trimmedLine)) continue;

                if (trimmedLine.StartsWith("PublicKeyHash"))
                {
                    var parts = trimmedLine.Split(new[] { ':' }, 2);
                    if (parts.Length == 2)
                    {
                        processedLines["PublicKeyHash"] = parts[1].Trim();
                    }
                }
                else if (trimmedLine.StartsWith("[Issuer]"))
                {
                    var nextLineIndex = Array.IndexOf(lines, line) + 1;
                    if (nextLineIndex < lines.Length)
                    {
                        processedLines["Issuer"] = lines[nextLineIndex].Trim();
                    }
                }
                else if (trimmedLine.StartsWith("[Serial Number]"))
                {
                    var nextLineIndex = Array.IndexOf(lines, line) + 1;
                    if (nextLineIndex < lines.Length)
                    {
                        processedLines["Serial Number"] = lines[nextLineIndex].Trim();
                    }
                }
                else if (trimmedLine.StartsWith("[Thumbprint]"))
                {
                    var nextLineIndex = Array.IndexOf(lines, line) + 1;
                    if (nextLineIndex < lines.Length)
                    {
                        processedLines["Thumbprint"] = lines[nextLineIndex].Trim();
                    }
                }
            }

            // Output in desired order
            if (processedLines.ContainsKey("PublicKeyHash"))
                sb.AppendLine($"Sha256 Hash: {processedLines["PublicKeyHash"]}");
            if (processedLines.ContainsKey("Serial Number"))
                sb.AppendLine($"Serial Number: {processedLines["Serial Number"]}");
            if (processedLines.ContainsKey("Thumbprint"))
                sb.AppendLine($"Thumbprint: {processedLines["Thumbprint"]}");
            if (processedLines.ContainsKey("Issuer"))
            {
                var issuerValue = processedLines["Issuer"];
                // Extract CN and O values
                var cnMatch = issuerValue.Contains("CN=") ? issuerValue.Split(new[] { "CN=" }, StringSplitOptions.None)[1].Split(',')[0].Trim() : "";
                var orgMatch = issuerValue.Contains("O=") ? issuerValue.Split(new[] { "O=" }, StringSplitOptions.None)[1].Split(',')[0].Trim() : "";
                
                var formattedIssuer = new List<string>();
                if (!string.IsNullOrEmpty(cnMatch)) formattedIssuer.Add($"CN={cnMatch}");
                if (!string.IsNullOrEmpty(orgMatch)) formattedIssuer.Add($"O={orgMatch}");
                
                if (formattedIssuer.Count > 0)
                    sb.AppendLine($"Issuer: {string.Join(", ", formattedIssuer)}");
            }
        }
        catch (Exception ex)
        {
            sb.AppendLine($"Unable to retrieve TPM information: {ex.Message}");
        }

        return sb.ToString();
    }
}