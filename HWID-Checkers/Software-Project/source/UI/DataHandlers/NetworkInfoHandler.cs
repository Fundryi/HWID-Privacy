using System;
using System.Collections.Generic;
using System.Linq;
using System.Windows.Forms;
using HWIDChecker.Hardware;
using HWIDChecker.UI.Components;

namespace HWIDChecker.UI.DataHandlers
{
    public class NetworkInfoHandler : HardwareDataHandler
    {
        public NetworkInfoHandler(HardwareInfoManager manager) : base(manager) { }

        public override void UpdateDisplay(RichTextBox textBox)
        {
            var info = hardwareInfoManager.GetSpecificHardwareInfo("NETWORK INFORMATION");
            var adapters = ParseNetworkAdapters(info);
            UpdateMultiItemDisplay(textBox, FormatAdapterInfo(adapters));
        }

        private List<Dictionary<string, string>> ParseNetworkAdapters(string info)
        {
            var adapters = new List<Dictionary<string, string>>();
            var currentAdapter = new Dictionary<string, string>();

            var lines = info.Split('\n')
                .Where(line => !string.IsNullOrWhiteSpace(line))
                .Where(line => !line.Trim().Equals("NETWORK INFORMATION"))
                .Select(line => line.Trim());

            foreach (var line in lines)
            {
                // Check if this line starts a new adapter
                if (line.StartsWith("Adapter Name:") || line.StartsWith("Network Adapter:"))
                {
                    if (currentAdapter.Count > 0)
                    {
                        adapters.Add(new Dictionary<string, string>(currentAdapter));
                        currentAdapter.Clear();
                    }

                    var parts = line.Split(new[] { ':' }, 2);
                    if (parts.Length == 2)
                    {
                        currentAdapter["Adapter Name"] = parts[1].Trim();
                    }
                }
                else if (line.Contains(":"))
                {
                    var parts = line.Split(new[] { ':' }, 2);
                    if (parts.Length == 2)
                    {
                        currentAdapter[parts[0].Trim()] = parts[1].Trim();
                    }
                }
            }

            // Add the last adapter if there is one
            if (currentAdapter.Count > 0)
            {
                adapters.Add(currentAdapter);
            }

            return adapters;
        }

        private List<Dictionary<string, string>> FormatAdapterInfo(List<Dictionary<string, string>> adapters)
        {
            var formattedAdapters = new List<Dictionary<string, string>>();

            foreach (var adapter in adapters)
            {
                var formattedAdapter = new Dictionary<string, string>();

                // Add adapter name first
                if (adapter.ContainsKey("Adapter Name"))
                {
                    formattedAdapter["Adapter"] = adapter["Adapter Name"];
                }

                // Add other properties in specific order
                var orderedProperties = new[]
                {
                    "MAC Address",
                    "Connection Type",
                    "IP Address",
                    "Subnet Mask",
                    "Default Gateway",
                    "DHCP Server",
                    "DNS Servers"
                };

                foreach (var prop in orderedProperties)
                {
                    if (adapter.ContainsKey(prop))
                    {
                        formattedAdapter[prop] = adapter[prop];
                    }
                }

                // Add any remaining properties not in the ordered list
                foreach (var kvp in adapter)
                {
                    if (!orderedProperties.Contains(kvp.Key) && kvp.Key != "Adapter Name")
                    {
                        formattedAdapter[kvp.Key] = kvp.Value;
                    }
                }

                formattedAdapters.Add(formattedAdapter);
            }

            return formattedAdapters;
        }
    }
}