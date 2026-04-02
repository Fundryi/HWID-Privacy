using System.Diagnostics;
using System.Management;
using System.Text;
using System.Text.RegularExpressions;
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
            if (TryGetTpmInfoFromWmi(out bool isPresent, out bool isEnabled, out bool isActivated,
                    out string manufacturer, out string version, out string specVersion))
            {
                if (!isPresent)
                {
                    sb.AppendLine("TPM OFF");
                    return sb.ToString();
                }

                sb.AppendLine(isEnabled ? "TPM: ENABLED" : "TPM: DISABLED");

                if (!string.IsNullOrEmpty(manufacturer))
                {
                    sb.AppendLine($"TPM Manufacturer: {manufacturer}");
                }
                if (!string.IsNullOrEmpty(version))
                {
                    sb.AppendLine($"TPM Version: {version}");
                }
                if (!string.IsNullOrEmpty(specVersion))
                {
                    sb.AppendLine($"TPM Spec Version: {specVersion}");
                }

                if (isEnabled)
                {
                    AppendEkCertificateInfo(sb);
                }

                return sb.ToString();
            }

            AppendPowerShellFallback(sb);
        }
        catch (Exception ex)
        {
            sb.AppendLine($"Unable to retrieve TPM information: {ex.Message}");
        }

        return sb.ToString();
    }

    private static bool TryGetTpmInfoFromWmi(out bool isPresent, out bool isEnabled, out bool isActivated,
        out string manufacturer, out string version, out string specVersion)
    {
        isPresent = false;
        isEnabled = false;
        isActivated = false;
        manufacturer = string.Empty;
        version = string.Empty;
        specVersion = string.Empty;

        try
        {
            var scope = new ManagementScope(@"\\.\root\cimv2\security\MicrosoftTpm");
            scope.Connect();

            using var searcher = new ManagementObjectSearcher(scope, new ObjectQuery("SELECT * FROM Win32_Tpm"));
            var tpm = searcher.Get().Cast<ManagementObject>().FirstOrDefault();
            if (tpm == null)
            {
                return false;
            }

            // If we got a Win32_Tpm object, the TPM exists. IsPresent() is not
            // implemented on all TPMs (e.g., Intel INTC), so don't gate on it.
            isPresent = true;
            isEnabled = TryInvokeBoolMethod(tpm, "IsEnabled");
            isActivated = TryInvokeBoolMethod(tpm, "IsActivated");

            string manufacturerTxt = tpm["ManufacturerIdTxt"]?.ToString() ?? string.Empty;
            manufacturer = !string.IsNullOrWhiteSpace(manufacturerTxt)
                ? manufacturerTxt
                : FormatManufacturerId(tpm["ManufacturerId"]);
            version = tpm["ManufacturerVersion"]?.ToString() ?? string.Empty;
            specVersion = tpm["SpecVersion"]?.ToString() ?? string.Empty;

            return true;
        }
        catch
        {
            return false;
        }
    }

    private static bool TryInvokeBoolMethod(ManagementObject tpm, string methodName)
    {
        try
        {
            // Win32_Tpm methods return uint (0=success) + bool out parameter.
            // Use the 3-param InvokeMethod to get the out parameters.
            var inParams = tpm.GetMethodParameters(methodName);
            var outParams = tpm.InvokeMethod(methodName, inParams, null);

            if (outParams != null)
            {
                // The out parameter name matches the method name
                try
                {
                    var value = outParams[methodName];
                    if (value != null)
                        return Convert.ToBoolean(value);
                }
                catch { }

                // Scan all bool properties as fallback (skip ReturnValue error code)
                foreach (PropertyData property in outParams.Properties)
                {
                    if (property.Name == "ReturnValue") continue;
                    if (property.Value is bool boolValue)
                        return boolValue;
                }
            }
        }
        catch
        {
            // Method may not be implemented on all TPMs (e.g., Intel INTC lacks IsPresent)
        }

        // Fallback: try reading the *_InitialValue property directly
        try
        {
            var value = tpm[$"{methodName}_InitialValue"];
            if (value != null)
                return Convert.ToBoolean(value);
        }
        catch { }

        return false;
    }

    private static string FormatManufacturerId(object manufacturerId)
    {
        if (manufacturerId == null)
        {
            return string.Empty;
        }

        if (uint.TryParse(manufacturerId.ToString(), out uint id))
        {
            return $"0x{id:X8}";
        }

        return manufacturerId.ToString() ?? string.Empty;
    }

    private static void AppendEkCertificateInfo(StringBuilder sb)
    {
        var output = RunPowerShellCommand("Get-TpmEndorsementKeyInfo -Hash 'Sha256' | Format-List");
        if (string.IsNullOrWhiteSpace(output))
        {
            sb.AppendLine("Unable to retrieve detailed TPM information");
            return;
        }

        var processedLines = ParseEkInfoOutput(output);

        if (processedLines.TryGetValue("PublicKeyHash", out var publicKeyHash))
        {
            sb.AppendLine($"Sha256 Hash: {publicKeyHash}");
        }
        if (processedLines.TryGetValue("Serial Number", out var serialNumber))
        {
            sb.AppendLine($"Serial Number: {serialNumber}");
        }
        if (processedLines.TryGetValue("Thumbprint", out var thumbprint))
        {
            sb.AppendLine($"Thumbprint: {thumbprint}");
        }
        if (processedLines.TryGetValue("Issuer", out var issuerValue))
        {
            var cnMatch = issuerValue.Contains("CN=")
                ? issuerValue.Split(new[] { "CN=" }, StringSplitOptions.None)[1].Split(',')[0].Trim()
                : string.Empty;
            var orgMatch = issuerValue.Contains("O=")
                ? issuerValue.Split(new[] { "O=" }, StringSplitOptions.None)[1].Split(',')[0].Trim()
                : string.Empty;

            var formattedIssuer = new List<string>();
            if (!string.IsNullOrEmpty(cnMatch)) formattedIssuer.Add($"CN={cnMatch}");
            if (!string.IsNullOrEmpty(orgMatch)) formattedIssuer.Add($"O={orgMatch}");

            if (formattedIssuer.Count > 0)
            {
                sb.AppendLine($"Issuer: {string.Join(", ", formattedIssuer)}");
            }
        }
    }

    private static void AppendPowerShellFallback(StringBuilder sb)
    {
        var tpmOutput = RunPowerShellCommand("Get-Tpm");
        if (string.IsNullOrWhiteSpace(tpmOutput))
        {
            sb.AppendLine("Unable to retrieve TPM information");
            return;
        }

        bool isPresent = Regex.IsMatch(tpmOutput, @"^TpmPresent\s*:\s*True\b", RegexOptions.Multiline);
        if (!isPresent)
        {
            sb.AppendLine("TPM OFF");
            return;
        }

        bool isEnabled = Regex.IsMatch(tpmOutput, @"^TpmEnabled\s*:\s*True\b", RegexOptions.Multiline);
        sb.AppendLine(isEnabled ? "TPM: ENABLED" : "TPM: DISABLED");

        if (isEnabled)
        {
            AppendEkCertificateInfo(sb);
        }
    }

    private static Dictionary<string, string> ParseEkInfoOutput(string output)
    {
        var processedLines = new Dictionary<string, string>(StringComparer.OrdinalIgnoreCase);
        var lines = output.Split(new[] { '\r', '\n' }, StringSplitOptions.RemoveEmptyEntries);
        string pendingSection = null;

        foreach (var line in lines)
        {
            var trimmedLine = line.Trim();
            if (string.IsNullOrWhiteSpace(trimmedLine)) continue;

            var hashMatch = Regex.Match(trimmedLine, @"^PublicKeyHash\s*:\s*(.+)$");
            if (hashMatch.Success)
            {
                processedLines["PublicKeyHash"] = hashMatch.Groups[1].Value.Trim();
                continue;
            }

            var sectionMatch = Regex.Match(trimmedLine, @"^\[(.+)\]$");
            if (sectionMatch.Success)
            {
                pendingSection = sectionMatch.Groups[1].Value.Trim();
                continue;
            }

            if (!string.IsNullOrEmpty(pendingSection))
            {
                processedLines[pendingSection] = trimmedLine;
                pendingSection = null;
            }
        }

        return processedLines;
    }

    private static string RunPowerShellCommand(string arguments)
    {
        var process = new Process
        {
            StartInfo = new ProcessStartInfo
            {
                FileName = "powershell",
                Arguments = $"-Command \"{arguments}\"",
                UseShellExecute = false,
                RedirectStandardOutput = true,
                CreateNoWindow = true
            }
        };

        process.Start();
        string output = process.StandardOutput.ReadToEnd();
        process.WaitForExit();

        return output;
    }
}
