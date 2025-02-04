using System;
using System.Collections.Generic;
using System.Linq;
using System.Text.RegularExpressions;
using HWIDChecker.Services.Interfaces;
using HWIDChecker.Services.Models;
using HWIDChecker.Services.Strategies;

namespace HWIDChecker.Services
{
    public class ComponentParser : IComponentParser
    {
        private readonly Dictionary<string, IComponentIdentifierStrategy> _strategies;
        private readonly Dictionary<string, string> _sectionTypeMap;

        public ComponentParser()
        {
            _sectionTypeMap = new Dictionary<string, string>
            {
                ["DISK DRIVE INFORMATION"] = "DISK DRIVE",
                ["RAM INFORMATION"] = "RAM",
                ["CPU INFORMATION"] = "CPU",
                ["MOTHERBOARD INFORMATION"] = "MOTHERBOARD",
                ["BIOS INFORMATION"] = "BIOS",
                ["GPU INFORMATION"] = "GPU",
                ["TPM INFORMATION"] = "TPM",
                ["USB INFORMATION"] = "USB",
                ["MONITOR INFORMATION"] = "MONITOR",
                ["NETWORK ADAPTERS (NIC's)"] = "NETWORK",
                ["ARP INFORMATION"] = "NETWORK"
            };

            _strategies = new Dictionary<string, IComponentIdentifierStrategy>
            {
                ["DISK DRIVE"] = new DiskDriveIdentifierStrategy(),
                ["RAM"] = new RamIdentifierStrategy(),
                ["CPU"] = new CpuIdentifierStrategy(),
                ["MOTHERBOARD"] = new MotherboardIdentifierStrategy(),
                ["BIOS"] = new BiosIdentifierStrategy(),
                ["GPU"] = new GpuIdentifierStrategy(),
                ["TPM"] = new TpmIdentifierStrategy(),
                ["USB"] = new UsbIdentifierStrategy(),
                ["MONITOR"] = new MonitorIdentifierStrategy(),
                ["NETWORK"] = new NetworkAdapterIdentifierStrategy()
            };
        }

        public List<ComponentIdentifier> Parse(string configurationData)
        {
            if (string.IsNullOrEmpty(configurationData))
            {
                throw new ArgumentException("Configuration data cannot be null or empty");
            }

            var components = new List<ComponentIdentifier>();
            var sections = SplitIntoSections(configurationData);

            foreach (var section in sections)
            {
                string componentType = null;
                try
                {
                    componentType = GetComponentType(section);
                    if (componentType == null) continue;

                    var properties = ParseProperties(section);
                    if (!properties.Any()) continue;

                    if (_strategies.TryGetValue(componentType, out var strategy))
                    {
                        var identifier = strategy.GetIdentifier(properties);
                        if (identifier == null)
                        {
                            var fallbacks = strategy.GetFallbackIdentifiers(properties);
                            identifier = fallbacks.FirstOrDefault();
                            if (identifier == null) continue;
                        }

                        var component = new ComponentIdentifier(componentType, identifier);
                        foreach (var prop in properties)
                        {
                            component.AddProperty(prop.Key, prop.Value);
                        }
                        components.Add(component);
                    }
                }
                catch (Exception ex)
                {
                    throw new ComponentParseException($"Error parsing section in {componentType ?? "unknown component"}", ex);
                }
            }

            return components;
        }

        private string GetComponentType(string sectionText)
        {
            var headerMatch = Regex.Match(sectionText, @"^([^\n]+)");
            if (!headerMatch.Success) return null;

            var header = headerMatch.Groups[1].Value.Trim();
            return _sectionTypeMap.TryGetValue(header, out var mappedType) ? mappedType : null;
        }

        private List<string> SplitIntoSections(string configText)
        {
            var sections = new List<string>();
            var currentSection = new List<string>();
            var lines = configText.Split('\n');

            foreach (var line in lines)
            {
                var trimmedLine = line.TrimEnd('\r');

                if (trimmedLine.EndsWith("INFORMATION") || trimmedLine.Contains("NIC's"))
                {
                    if (currentSection.Count > 0)
                    {
                        sections.Add(string.Join("\n", currentSection));
                        currentSection.Clear();
                    }
                }

                currentSection.Add(trimmedLine);
            }

            if (currentSection.Count > 0)
            {
                sections.Add(string.Join("\n", currentSection));
            }

            return sections;
        }

        private Dictionary<string, string> ParseProperties(string sectionText)
        {
            var properties = new Dictionary<string, string>();
            var lines = sectionText.Split('\n');
            var propertyPattern = @"^([^:]+):\s*(.+)$";

            foreach (var line in lines)
            {
                var match = Regex.Match(line, propertyPattern);
                if (match.Success)
                {
                    var key = match.Groups[1].Value.Trim();
                    var value = match.Groups[2].Value.Trim();
                    properties[key] = value;
                }
            }

            return properties;
        }
    }

    public class ComponentParseException : Exception
    {
        public ComponentParseException(string message, Exception innerException = null)
            : base(message, innerException)
        {
        }
    }
}
