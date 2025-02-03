using System;
using System.Collections.Generic;
using System.Linq;
using System.Text.RegularExpressions;
using System.Threading.Tasks;
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
            try
            {
                System.Diagnostics.Debug.WriteLine("Initializing ComponentParser...");
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
                    ["ARP INFORMATION"] = "NETWORK"  // Map ARP info to network type
                };

                System.Diagnostics.Debug.WriteLine("Creating strategies...");
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
                System.Diagnostics.Debug.WriteLine("ComponentParser initialized successfully");
            }
            catch (Exception ex)
            {
                System.Diagnostics.Debug.WriteLine($"Error initializing ComponentParser: {ex}");
                throw new Exception("Failed to initialize ComponentParser", ex);
            }
        }

        public async Task<List<ComponentIdentifier>> ParseConfiguration(string configText)
        {
            try
            {
                System.Diagnostics.Debug.WriteLine("Starting to parse configuration...");
                if (string.IsNullOrEmpty(configText))
                {
                    System.Diagnostics.Debug.WriteLine("Configuration text is null or empty");
                    throw new ArgumentException("Configuration text cannot be null or empty");
                }

                return await Task.Run(() =>
                {
                    var components = new List<ComponentIdentifier>();
                    var sections = SplitIntoSections(configText);
                    System.Diagnostics.Debug.WriteLine($"Split configuration into {sections.Count} sections");

                    foreach (var section in sections)
                    {
                        try
                        {
                            var componentType = GetComponentType(section);
                            if (componentType == null)
                            {
                                System.Diagnostics.Debug.WriteLine($"Could not determine component type for section: {section.Split('\n')[0]}");
                                continue;
                            }

                            var properties = ParseProperties(section);
                            if (!properties.Any())
                            {
                                System.Diagnostics.Debug.WriteLine($"No properties found for component type: {componentType}");
                                continue;
                            }

                            if (_strategies.TryGetValue(componentType, out var strategy))
                            {
                                System.Diagnostics.Debug.WriteLine($"Processing {componentType} with {properties.Count} properties");
                                var identifier = strategy.GetIdentifier(properties);
                                if (identifier == null)
                                {
                                    var fallbacks = strategy.GetFallbackIdentifiers(properties);
                                    identifier = fallbacks.FirstOrDefault();
                                    if (identifier == null)
                                    {
                                        System.Diagnostics.Debug.WriteLine($"No identifier found for {componentType}");
                                        continue;
                                    }
                                }

                                var component = new ComponentIdentifier(componentType, identifier);
                                foreach (var prop in properties)
                                {
                                    component.AddProperty(prop.Key, prop.Value);
                                }
                                components.Add(component);
                                System.Diagnostics.Debug.WriteLine($"Successfully added component: {componentType}");
                            }
                            else
                            {
                                System.Diagnostics.Debug.WriteLine($"No strategy found for component type: {componentType}");
                            }
                        }
                        catch (Exception ex)
                        {
                            System.Diagnostics.Debug.WriteLine($"Error processing section: {ex}");
                            // Continue processing other sections
                        }
                    }

                    System.Diagnostics.Debug.WriteLine($"Parsed {components.Count} components successfully");
                    return components;
                });
            }
            catch (Exception ex)
            {
                System.Diagnostics.Debug.WriteLine($"Error parsing configuration: {ex}");
                throw new Exception("Failed to parse configuration", ex);
            }
        }

        public string GetComponentType(string sectionText)
        {
            try
            {
                var headerMatch = Regex.Match(sectionText, @"^([^\n]+)");
                if (!headerMatch.Success)
                {
                    System.Diagnostics.Debug.WriteLine("No header found in section");
                    return null;
                }

                var header = headerMatch.Groups[1].Value.Trim();
                var type = _sectionTypeMap.TryGetValue(header, out var mappedType) ? mappedType : null;
                System.Diagnostics.Debug.WriteLine($"Header '{header}' mapped to type: {type ?? "null"}");
                return type;
            }
            catch (Exception ex)
            {
                System.Diagnostics.Debug.WriteLine($"Error getting component type: {ex}");
                return null;
            }
        }

        private List<string> SplitIntoSections(string configText)
        {
            try
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

                System.Diagnostics.Debug.WriteLine($"Split configuration into {sections.Count} sections");
                return sections;
            }
            catch (Exception ex)
            {
                System.Diagnostics.Debug.WriteLine($"Error splitting sections: {ex}");
                throw;
            }
        }

        private Dictionary<string, string> ParseProperties(string sectionText)
        {
            try
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

                System.Diagnostics.Debug.WriteLine($"Parsed {properties.Count} properties from section");
                return properties;
            }
            catch (Exception ex)
            {
                System.Diagnostics.Debug.WriteLine($"Error parsing properties: {ex}");
                throw;
            }
        }
    }
}