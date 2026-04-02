using System.Management;
using System.Text;
using HWIDChecker.Services;
using HWIDChecker.Services.Win32;
using Microsoft.Win32;

namespace HWIDChecker.Hardware;

public class NetworkInfo : IHardwareInfo
{
    private readonly TextFormattingService _textFormatter;

    public string SectionTitle => "NETWORK ADAPTERS (NIC's)";

    public NetworkInfo(TextFormattingService textFormatter)
    {
        _textFormatter = textFormatter;
    }

    private string SimplifyAdapterType(string adapterType)
    {
        if (string.IsNullOrEmpty(adapterType))
            return "Unknown";

        adapterType = adapterType.ToUpper();

        if (adapterType.Contains("802.11") || adapterType.Contains("WIRELESS") ||
            adapterType.Contains("WI-FI") || adapterType.Contains("WIFI"))
            return "WiFi";

        if (adapterType.Contains("802.3") || adapterType.Contains("ETHERNET"))
            return "Ethernet";

        if (adapterType.Contains("BLUETOOTH"))
            return "Bluetooth";

        return adapterType;
    }

    private bool IsRealNetworkAdapter(ManagementObject nic)
    {
        // Check if it's a physical adapter
        if (nic["PhysicalAdapter"] != null && (bool)nic["PhysicalAdapter"] == false)
            return false;

        string pnpDeviceId = nic["PNPDeviceID"]?.ToString()?.ToUpper() ?? "";
        string productName = nic["ProductName"]?.ToString()?.ToUpper() ?? "";
        string adapterType = nic["AdapterType"]?.ToString()?.ToUpper() ?? "";
        string name = nic["Name"]?.ToString()?.ToUpper() ?? "";

        // Check PNPDeviceID for PCIe, USB, and Mellanox identifiers
        bool isMellanox = pnpDeviceId.StartsWith("MLX4\\") || pnpDeviceId.StartsWith("MLX5\\");
        bool isPCIeOrUSB = pnpDeviceId.StartsWith("PCI\\") ||
                          pnpDeviceId.StartsWith("USB\\") ||
                          isMellanox ||
                          pnpDeviceId.Contains("PCI_") ||
                          pnpDeviceId.Contains("USB_");

        // Comprehensive check for virtual adapters
        string[] virtualKeywords = {
            "VIRTUAL", "VPN", "TAP", "TUN", "TUNNEL",
            "VMWARE", "HYPER-V", "VIRTUALBOX", "CISCO",
            "CHECKPOINT", "FORTINET", "JUNIPER", "CITRIX",
            "SOFTETHER", "OPENVPN", "WIREGUARD", "GHOST",
            "HAMACHI", "NDIS", "BRIDGE", "LOOPBACK"
        };

        bool isVirtual = virtualKeywords.Any(keyword =>
            productName.Contains(keyword) || name.Contains(keyword));

        // Check for physical adapter types
        bool isPhysicalType = adapterType.Contains("ETHERNET") ||
                             adapterType.Contains("802.3") ||
                             adapterType.Contains("WIRELESS") ||
                             adapterType.Contains("WI-FI") ||
                             adapterType.Contains("WIFI") ||
                             adapterType.Contains("802.11");

        // Must be PCIe/USB based AND not virtual AND a physical type
        return isPCIeOrUSB && !isVirtual && (isPhysicalType || isMellanox);
    }

    public string GetInformation()
    {
        var sb = new StringBuilder();
        using var searcher = new ManagementObjectSearcher("SELECT * FROM Win32_NetworkAdapter");
        var realAdapters = new List<ManagementObject>();
        Dictionary<string, string> hwIdMap = null;

        try { hwIdMap = SetupApi.GetHardwareIdMap(); } catch { }

        foreach (ManagementObject nic in searcher.Get())
        {
            if (nic["MACAddress"] != null && IsRealNetworkAdapter(nic))
            {
                realAdapters.Add(nic);
            }
        }

        for (int i = 0; i < realAdapters.Count; i++)
        {
            var nic = realAdapters[i];
            var currentMac = nic["MACAddress"]?.ToString() ?? "";
            var pnpDeviceId = nic["PNPDeviceID"]?.ToString() ?? "";

            var adapterInfo = new List<(string, string)>
            {
                ("Name", nic["Name"]?.ToString() ?? ""),
                ("Product Name", nic["ProductName"]?.ToString() ?? ""),
                ("Device ID", nic["DeviceID"]?.ToString() ?? ""),
                ("Adapter Type", SimplifyAdapterType(nic["AdapterType"]?.ToString()))
            };

            // Add SetupAPI Hardware ID if available
            if (hwIdMap != null && !string.IsNullOrEmpty(pnpDeviceId) &&
                hwIdMap.TryGetValue(pnpDeviceId, out var hwId))
            {
                adapterInfo.Add(("Hardware ID", hwId));
            }

            // Check if MAC has been overridden via registry
            var overrideMac = GetRegistryMacOverride(pnpDeviceId);
            if (!string.IsNullOrEmpty(overrideMac))
            {
                // MAC is spoofed — current MAC is the override, permanent is the real one
                adapterInfo.Add(("MAC Address (Overridden)", currentMac));
                adapterInfo.Add(("Permanent MAC", GetPermanentMac(pnpDeviceId, currentMac)));
            }
            else
            {
                adapterInfo.Add(("MAC Address", currentMac));
            }

            foreach (var info in adapterInfo)
            {
                _textFormatter.AppendInfoLine(sb, info.Item1, info.Item2);
            }

            // Add separator if not the last adapter
            if (i < realAdapters.Count - 1)
            {
                _textFormatter.AppendItemSeparator(sb);
            }
        }

        return sb.ToString();
    }

    /// <summary>
    /// Checks if a NIC has a NetworkAddress registry override (MAC spoof).
    /// Returns the override MAC string if set, null otherwise.
    /// </summary>
    private string GetRegistryMacOverride(string pnpDeviceId)
    {
        if (string.IsNullOrEmpty(pnpDeviceId))
            return null;

        try
        {
            // Network adapters are under this class GUID in the registry
            const string netClassKey = @"SYSTEM\CurrentControlSet\Control\Class\{4d36e972-e325-11ce-bfc1-08002be10318}";
            using var classKey = Registry.LocalMachine.OpenSubKey(netClassKey);
            if (classKey == null) return null;

            foreach (var subKeyName in classKey.GetSubKeyNames())
            {
                // Skip non-numeric entries like "Properties"
                if (!int.TryParse(subKeyName, out _)) continue;

                using var subKey = classKey.OpenSubKey(subKeyName);
                if (subKey == null) continue;

                // Match by MatchingDeviceId or DeviceInstanceID
                var instanceId = subKey.GetValue("DeviceInstanceID")?.ToString();
                var matchingId = subKey.GetValue("MatchingDeviceId")?.ToString();

                bool isMatch = false;
                if (!string.IsNullOrEmpty(instanceId))
                    isMatch = string.Equals(instanceId, pnpDeviceId, StringComparison.OrdinalIgnoreCase);
                if (!isMatch && !string.IsNullOrEmpty(matchingId))
                    isMatch = pnpDeviceId.StartsWith(matchingId, StringComparison.OrdinalIgnoreCase);

                if (!isMatch) continue;

                var networkAddress = subKey.GetValue("NetworkAddress")?.ToString();
                if (!string.IsNullOrEmpty(networkAddress))
                {
                    return FormatMacAddress(networkAddress);
                }

                // No NetworkAddress on this entry — keep checking other subkeys
                // (stale driver entries may precede the active one)
            }
        }
        catch
        {
            // Registry access failed — not critical
        }

        return null;
    }

    /// <summary>
    /// Returns a label indicating the MAC is spoofed.
    /// WMI cannot retrieve the burned-in MAC when a registry override is active —
    /// the main value here is that we detected the spoof via the NetworkAddress key.
    /// </summary>
    private static string GetPermanentMac(string pnpDeviceId, string currentMac)
    {
        return "Spoofed (see NetworkAddress registry override)";
    }

    /// <summary>
    /// Formats a raw MAC string (e.g. "001122334455") into colon-separated form.
    /// </summary>
    private static string FormatMacAddress(string raw)
    {
        if (string.IsNullOrEmpty(raw)) return raw;
        // Remove any existing separators
        raw = raw.Replace("-", "").Replace(":", "").Replace(".", "");
        if (raw.Length != 12) return raw;

        return string.Join(":", Enumerable.Range(0, 6).Select(i => raw.Substring(i * 2, 2).ToUpper()));
    }
}
