using System;
using System.Collections.Generic;
using System.Linq;
using System.Windows.Forms;
using HWIDChecker.Hardware;
using HWIDChecker.UI.Components;

namespace HWIDChecker.UI.DataHandlers
{
    public abstract class HardwareDataHandler
    {
        protected readonly HardwareInfoManager hardwareInfoManager;

        protected HardwareDataHandler(HardwareInfoManager manager)
        {
            hardwareInfoManager = manager;
        }

        public abstract void UpdateDisplay(RichTextBox textBox);

        protected Dictionary<string, string> ParseBasicInfo(string info)
        {
            var result = new Dictionary<string, string>();
            var lines = info.Split('\n')
                .Where(line => !string.IsNullOrWhiteSpace(line))
                .Select(line => line.Trim());

            foreach (var line in lines)
            {
                if (line.Contains(":"))
                {
                    var parts = line.Split(new[] { ':' }, 2);
                    if (parts.Length == 2)
                    {
                        result[parts[0].Trim()] = parts[1].Trim();
                    }
                }
            }

            return result;
        }

        protected List<Dictionary<string, string>> ParseMultiItemInfo(string info)
        {
            var items = new List<Dictionary<string, string>>();
            var currentItem = new Dictionary<string, string>();

            var lines = info.Split('\n')
                .Where(line => !string.IsNullOrWhiteSpace(line))
                .Select(line => line.Trim());

            foreach (var line in lines)
            {
                if (line.Contains(":"))
                {
                    var parts = line.Split(new[] { ':' }, 2);
                    if (parts.Length == 2)
                    {
                        // If we find a key that's already in the dictionary, it's a new item
                        if (currentItem.ContainsKey(parts[0].Trim()))
                        {
                            if (currentItem.Count > 0)
                            {
                                items.Add(new Dictionary<string, string>(currentItem));
                                currentItem.Clear();
                            }
                        }
                        currentItem[parts[0].Trim()] = parts[1].Trim();
                    }
                }
            }

            if (currentItem.Count > 0)
            {
                items.Add(currentItem);
            }

            return items;
        }

        protected void UpdateSingleItemDisplay(RichTextBox textBox, Dictionary<string, string> data)
        {
            textBox.Clear();
            foreach (var kvp in data)
            {
                textBox.AppendText($"{kvp.Key}: {kvp.Value}\n");
            }
        }

        protected void UpdateMultiItemDisplay(RichTextBox textBox, List<Dictionary<string, string>> items)
        {
            textBox.Clear();
            foreach (var item in items)
            {
                foreach (var kvp in item)
                {
                    textBox.AppendText($"{kvp.Key}: {kvp.Value}\n");
                }
                textBox.AppendText("\n"); // Add blank line between items
            }
        }
    }
}