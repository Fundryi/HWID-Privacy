using System;
using System.Collections.Generic;
using System.Drawing;
using System.Linq;
using System.Text.RegularExpressions;
using System.Windows.Forms;

namespace HWIDChecker.Services
{
    [Obsolete("Compare feature is currently disabled and will be reworked in a future update.")]
    public class CompareService
    {
        private static readonly Color DifferenceColor = Color.FromArgb(80, 30, 30);

        // Match the order from HardwareInfoManager.InitializeProviders()
        private static readonly List<string> SectionOrder = new List<string>
        {
            "DISK DRIVE INFORMATION",
            "CPU INFORMATION",
            "SYSTEM INFORMATION",
            "MOTHERBOARD INFORMATION",
            "BIOS INFORMATION",
            "RAM INFORMATION",
            "TPM INFORMATION",
            "GPU INFORMATION",
            "USB INFORMATION",
            "MONITOR INFORMATION",
            "NETWORK ADAPTERS (NIC's)",
            "ARP INFORMATION"
        };

        private struct PropertyInfo
        {
            public string Value { get; set; }
            public bool IsIdentifier { get; set; }

            public PropertyInfo(string value, bool isIdentifier)
            {
                Value = value;
                IsIdentifier = isIdentifier;
            }
        }

        private class HardwareSection
        {
            public string Title { get; set; }
            public List<Dictionary<string, PropertyInfo>> Items { get; set; }
            public int OrderIndex { get; set; }
        }

        public void HighlightDifferences(RichTextBox leftText, RichTextBox rightText)
        {
            leftText.Clear();
            rightText.Clear();

            leftText.SelectionFont = new Font(leftText.Font.FontFamily, 10, FontStyle.Bold);
            rightText.SelectionFont = new Font(rightText.Font.FontFamily, 10, FontStyle.Bold);

            const string message = "Compare feature is currently disabled.\nThis feature will be reworked in a future update.";

            leftText.AppendText(message);
            rightText.AppendText(message);

            leftText.SelectionStart = 0;
            rightText.SelectionStart = 0;
        }

        private List<HardwareSection> ParseSections(string text)
        {
            var sections = new List<HardwareSection>();
            var lines = text.Split('\n');
            HardwareSection currentSection = null;
            Dictionary<string, PropertyInfo> currentItem = null;

            foreach (var line in lines)
            {
                var trimmedLine = line.TrimEnd('\r');

                // Check for section header
                if (trimmedLine.EndsWith("INFORMATION") || trimmedLine.Contains("NIC's"))
                {
                    if (currentSection != null && currentItem != null)
                    {
                        currentSection.Items.Add(currentItem);
                    }

                    currentSection = new HardwareSection
                    {
                        Title = trimmedLine,
                        Items = new List<Dictionary<string, PropertyInfo>>(),
                        OrderIndex = SectionOrder.IndexOf(trimmedLine)
                    };
                    sections.Add(currentSection);
                    currentItem = null;
                    continue;
                }

                if (currentSection == null) continue;

                // Check for item separator
                if (trimmedLine.StartsWith("----------------------------------------"))
                {
                    if (currentItem != null)
                    {
                        currentSection.Items.Add(currentItem);
                        currentItem = null;
                    }
                    continue;
                }

                // Parse property lines
                var match = Regex.Match(trimmedLine, @"^([^:]+):\s*(.+)$");
                if (match.Success)
                {
                    var key = match.Groups[1].Value.Trim();
                    var value = match.Groups[2].Value.Trim();

                    if (currentItem == null)
                    {
                        currentItem = new Dictionary<string, PropertyInfo>();
                    }

                    // Mark identifiers based on section type and property
                    bool isIdentifier = IsIdentifierProperty(currentSection.Title, key);
                    currentItem[key] = new PropertyInfo(value, isIdentifier);
                }
            }

            // Add final item if exists
            if (currentSection != null && currentItem != null)
            {
                currentSection.Items.Add(currentItem);
            }

            return sections;
        }

        private bool IsIdentifierProperty(string sectionTitle, string propertyName)
        {
            if (sectionTitle.Contains("MONITOR"))
            {
                return propertyName == "Serial Number" || propertyName == "Product Code";
            }
            else if (sectionTitle.Contains("NIC"))
            {
                return propertyName == "MAC Address";
            }
            else if (sectionTitle.Contains("BIOS"))
            {
                return propertyName == "Serial Number";
            }
            else if (sectionTitle.Contains("MOTHERBOARD"))
            {
                return propertyName == "Serial Number" || propertyName == "Product ID";
            }
            else if (sectionTitle.Contains("CPU"))
            {
                return propertyName == "ProcessorId" || propertyName == "Serial Number";
            }
            else if (sectionTitle.Contains("DISK"))
            {
                return propertyName == "Serial Number";
            }
            else if (sectionTitle.Contains("GPU"))
            {
                return propertyName == "Device ID";
            }

            return false;
        }

        private void CompareAndHighlightSection(RichTextBox leftText, RichTextBox rightText,
            HardwareSection leftSection, HardwareSection rightSection)
        {
            // Add section header with proper formatting
            AppendSectionHeader(leftText, leftSection.Title);
            AppendSectionHeader(rightText, rightSection.Title);

            // Compare items in order (position-based comparison)
            int maxItems = Math.Max(leftSection.Items.Count, rightSection.Items.Count);

            for (int i = 0; i < maxItems; i++)
            {
                var leftItem = i < leftSection.Items.Count ? leftSection.Items[i] : null;
                var rightItem = i < rightSection.Items.Count ? rightSection.Items[i] : null;

                if (leftItem != null && rightItem != null)
                {
                    // Compare and highlight properties
                    var allProperties = leftItem.Keys.Union(rightItem.Keys).ToList();
                    foreach (var property in allProperties)
                    {
                        var leftValue = leftItem.GetValueOrDefault(property, new PropertyInfo("", false));
                        var rightValue = rightItem.GetValueOrDefault(property, new PropertyInfo("", false));

                        AppendProperty(leftText, property, leftValue.Value,
                            leftValue.Value != rightValue.Value && leftValue.IsIdentifier);
                        AppendProperty(rightText, property, rightValue.Value,
                            leftValue.Value != rightValue.Value && rightValue.IsIdentifier);
                    }
                }
                else if (leftItem != null)
                {
                    // Item removed
                    foreach (var kvp in leftItem)
                    {
                        AppendProperty(leftText, kvp.Key, kvp.Value.Value, true);
                        AppendProperty(rightText, kvp.Key, "", true);
                    }
                }
                else if (rightItem != null)
                {
                    // Item added
                    foreach (var kvp in rightItem)
                    {
                        AppendProperty(leftText, kvp.Key, "", true);
                        AppendProperty(rightText, kvp.Key, kvp.Value.Value, true);
                    }
                }

                // Add separator between items
                AppendSeparator(leftText);
                AppendSeparator(rightText);
            }

            // Add extra spacing between sections
            leftText.AppendText("\n");
            rightText.AppendText("\n");
        }

        private void AppendHeader(RichTextBox textBox, string title)
        {
            textBox.SelectionFont = new Font(textBox.Font.FontFamily, 12, FontStyle.Bold);
            textBox.AppendText($"{title}\n");
            textBox.SelectionFont = textBox.Font;
            textBox.AppendText(new string('=', 50) + "\n\n");
        }

        private void AppendSectionHeader(RichTextBox textBox, string title)
        {
            textBox.SelectionFont = new Font(textBox.Font.FontFamily, 10, FontStyle.Bold);
            textBox.AppendText($"{title}\n");
            textBox.SelectionFont = textBox.Font;
            AppendSeparator(textBox);
        }

        private void AppendProperty(RichTextBox textBox, string property, string value, bool highlight)
        {
            // Add property name without highlighting
            textBox.SelectionFont = new Font(textBox.Font.FontFamily, textBox.Font.Size, FontStyle.Bold);
            textBox.AppendText($"{property}: ");
            textBox.SelectionFont = textBox.Font;

            // Add value with highlighting if needed
            if (highlight)
            {
                textBox.SelectionBackColor = DifferenceColor;
            }
            textBox.AppendText(value);
            if (highlight)
            {
                textBox.SelectionBackColor = textBox.BackColor;
            }
            textBox.AppendText("\n");
        }

        private void AppendSeparator(RichTextBox textBox)
        {
            textBox.AppendText(new string('-', 50) + "\n");
        }

        private void AppendSection(RichTextBox textBox, HardwareSection section, bool highlight)
        {
            AppendSectionHeader(textBox, section.Title);
            foreach (var item in section.Items)
            {
                foreach (var kvp in item)
                {
                    AppendProperty(textBox, kvp.Key, kvp.Value.Value, highlight);
                }
                AppendSeparator(textBox);
            }
            textBox.AppendText("\n");
        }

        private void AppendEmptySection(RichTextBox textBox, string title)
        {
            AppendSectionHeader(textBox, title);
            AppendSeparator(textBox);
            textBox.AppendText("\n");
        }
    }
}