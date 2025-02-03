using System.Collections.Generic;
using System.Linq;
using System.Windows.Forms;
using HWIDChecker.Hardware;
using HWIDChecker.UI.Components;

namespace HWIDChecker.UI.DataHandlers
{
    public class SystemInfoHandler : HardwareDataHandler
    {
        public SystemInfoHandler(HardwareInfoManager manager) : base(manager) { }

        public override void UpdateDisplay(RichTextBox textBox)
        {
            var info = hardwareInfoManager.GetSpecificHardwareInfo("SYSTEM INFORMATION");
            var data = ParseSystemInfo(info);
            UpdateSingleItemDisplay(textBox, data);
        }

        private Dictionary<string, string> ParseSystemInfo(string info)
        {
            var result = new Dictionary<string, string>();

            // Get all lines except the title
            var lines = info.Split('\n')
                .Where(line => !string.IsNullOrWhiteSpace(line))
                .Where(line => !line.Trim().Equals("SYSTEM INFORMATION"))
                .Select(line => line.Trim())
                .ToList();

            // Create organized data with specific order
            foreach (var line in lines)
            {
                if (line.Contains(":"))
                {
                    var parts = line.Split(new[] { ':' }, 2);
                    if (parts.Length == 2)
                    {
                        var key = parts[0].Trim();
                        var value = parts[1].Trim();

                        // Handle specific items differently if needed
                        if (key.Contains("UUID"))
                        {
                            key = "System UUID";
                        }
                        else if (key.Contains("Product Key"))
                        {
                            key = "Windows Key";
                        }

                        result[key] = value;
                    }
                }
            }

            return result;
        }
    }
}