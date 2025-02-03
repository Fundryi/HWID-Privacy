using System.Management;
using System.Text;
using HWIDChecker.Services;

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

        // Check PNPDeviceID for PCIe and USB identifiers
        bool isPCIeOrUSB = pnpDeviceId.StartsWith("PCI\\") ||
                          pnpDeviceId.StartsWith("USB\\") ||
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
        return isPCIeOrUSB && !isVirtual && isPhysicalType;
    }

    public string GetInformation()
    {
        var sb = new StringBuilder();
        using var searcher = new ManagementObjectSearcher("SELECT * FROM Win32_NetworkAdapter");
        var realAdapters = new List<ManagementObject>();

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
            var adapterInfo = new[]
            {
                ("Name", nic["Name"]?.ToString() ?? ""),
                ("Product Name", nic["ProductName"]?.ToString() ?? ""),
                ("Device ID", nic["DeviceID"]?.ToString() ?? ""),
                ("Adapter Type", SimplifyAdapterType(nic["AdapterType"]?.ToString())),
                ("MAC Address", nic["MACAddress"]?.ToString() ?? "")
            };

            foreach (var info in adapterInfo)
            {
                _textFormatter.AppendInfoLine(sb, info.Item1, info.Item2);
            }

            // Add separator if not the last adapter
            if (i < realAdapters.Count - 1)
            {
                sb.AppendLine();
                _textFormatter.AppendItemSeparator(sb);
            }
        }

        return sb.ToString();
    }
}