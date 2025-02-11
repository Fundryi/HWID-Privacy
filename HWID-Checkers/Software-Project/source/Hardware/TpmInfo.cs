using System.Diagnostics;
using System.Text;

namespace HWIDChecker.Hardware;

public class TpmInfo : IHardwareInfo
{
    public string SectionTitle => "TPM MODULES";

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

            if (string.IsNullOrWhiteSpace(tpmOutput) || !tpmOutput.Contains("TpmPresent") || 
                (tpmOutput.Contains("TpmPresent") && !tpmOutput.Contains("True")))
            {
                sb.AppendLine("TPM OFF");
                return sb.ToString();
            }

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
                else if (trimmedLine.StartsWith("TPMVersion"))
                {
                    processedLines["TPMVersion"] = trimmedLine;
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
                sb.AppendLine($"PublicKeyHash: {processedLines["PublicKeyHash"]}");
            if (processedLines.ContainsKey("Serial Number"))
                sb.AppendLine($"Serial Number: {processedLines["Serial Number"]}");
            if (processedLines.ContainsKey("Thumbprint"))
                sb.AppendLine($"Thumbprint: {processedLines["Thumbprint"]}");
            if (processedLines.ContainsKey("TPMVersion"))
                sb.AppendLine(processedLines["TPMVersion"]); // TPMVersion already includes the full line
            if (processedLines.ContainsKey("Issuer"))
                sb.AppendLine($"Issuer: {processedLines["Issuer"]}");
        }
        catch (Exception ex)
        {
            sb.AppendLine($"Unable to retrieve TPM information: {ex.Message}");
        }

        return sb.ToString();
    }
}