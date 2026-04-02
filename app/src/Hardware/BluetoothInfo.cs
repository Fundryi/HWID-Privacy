using System.Management;
using System.Text;
using HWIDChecker.Services;
using Microsoft.Win32;

namespace HWIDChecker.Hardware;

public class BluetoothInfo : IHardwareInfo
{
    private readonly TextFormattingService _textFormatter;

    public string SectionTitle => "BLUETOOTH ADAPTERS";

    public BluetoothInfo(TextFormattingService textFormatter)
    {
        _textFormatter = textFormatter;
    }

    public string GetInformation()
    {
        var sb = new StringBuilder();
        var adapters = new List<(string Name, string Mac)>();

        // Method 1: WMI — find Bluetooth radio devices via PnP
        try
        {
            using var searcher = new ManagementObjectSearcher(
                "SELECT Name, PNPDeviceID FROM Win32_PnPEntity WHERE PNPDeviceID LIKE 'USB%' AND Name LIKE '%Bluetooth%'");

            foreach (ManagementObject device in searcher.Get())
            {
                var name = device["Name"]?.ToString() ?? "Unknown Bluetooth Adapter";
                var pnpId = device["PNPDeviceID"]?.ToString() ?? "";

                // Try to extract MAC from the BTHPORT registry
                var mac = GetBluetoothMacFromRegistry();
                if (!string.IsNullOrEmpty(mac))
                {
                    adapters.Add((name, mac));
                }
                else
                {
                    adapters.Add((name, "MAC not available"));
                }
            }
        }
        catch { }

        // Method 2: If WMI found nothing, try the BTHPORT service registry directly
        if (adapters.Count == 0)
        {
            try
            {
                var mac = GetBluetoothMacFromRegistry();
                if (!string.IsNullOrEmpty(mac))
                {
                    adapters.Add(("Bluetooth Adapter", mac));
                }
            }
            catch { }
        }

        // Method 3: Try WMI Bluetooth class as last resort
        if (adapters.Count == 0)
        {
            try
            {
                using var searcher = new ManagementObjectSearcher(
                    "SELECT Name, PNPDeviceID FROM Win32_PnPEntity WHERE Service = 'BTHUSB'");

                foreach (ManagementObject device in searcher.Get())
                {
                    var name = device["Name"]?.ToString() ?? "Unknown Bluetooth Adapter";
                    adapters.Add((name, "MAC not available"));
                }
            }
            catch { }
        }

        if (adapters.Count == 0)
        {
            sb.AppendLine("No Bluetooth adapters detected.");
            return sb.ToString();
        }

        for (int i = 0; i < adapters.Count; i++)
        {
            _textFormatter.AppendInfoLine(sb, "Adapter", adapters[i].Name);
            _textFormatter.AppendInfoLine(sb, "MAC Address", adapters[i].Mac);

            if (i < adapters.Count - 1)
            {
                _textFormatter.AppendItemSeparator(sb);
            }
        }

        return sb.ToString();
    }

    /// <summary>
    /// Reads the local Bluetooth adapter MAC from the BTHPORT registry key.
    /// The adapter's own address is stored as a REG_BINARY at LocalRadioAddress.
    /// </summary>
    private static string GetBluetoothMacFromRegistry()
    {
        try
        {
            using var key = Registry.LocalMachine.OpenSubKey(
                @"SYSTEM\CurrentControlSet\Services\BTHPORT\Parameters\Bluetooth Host Controller");
            if (key == null) return null;

            // Try LocalRadioAddress (6-byte binary, stored in little-endian)
            if (key.GetValue("LocalRadioAddress") is byte[] macBytes && macBytes.Length >= 6)
            {
                // Bluetooth MAC is stored in reverse byte order
                return string.Join(":", macBytes.Reverse().Select(b => b.ToString("X2")));
            }
        }
        catch { }

        // Fallback: enumerate paired devices to at least confirm BT is present
        try
        {
            using var devicesKey = Registry.LocalMachine.OpenSubKey(
                @"SYSTEM\CurrentControlSet\Services\BTHPORT\Parameters\Devices");
            if (devicesKey == null) return null;

            // Each subkey name is a Bluetooth address (12 hex chars) of a paired device
            // We want the adapter's own address, not paired devices
            // If we got here, BT stack is present but we couldn't get the local address
        }
        catch { }

        return null;
    }
}
