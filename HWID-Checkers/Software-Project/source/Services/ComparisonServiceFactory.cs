using System;
using System.Collections.Generic;
using HWIDChecker.Services.Interfaces;
using HWIDChecker.Services.Strategies;

namespace HWIDChecker.Services
{
    public class ComparisonServiceFactory
    {
        private static Dictionary<string, IComponentIdentifierStrategy> CreateStrategies()
        {
            try
            {
                System.Diagnostics.Debug.WriteLine("Creating component identifier strategies...");
                var strategies = new Dictionary<string, IComponentIdentifierStrategy>
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
                System.Diagnostics.Debug.WriteLine($"Created {strategies.Count} strategies successfully");
                return strategies;
            }
            catch (Exception ex)
            {
                System.Diagnostics.Debug.WriteLine($"Error creating strategies: {ex}");
                throw new Exception("Failed to create component identifier strategies", ex);
            }
        }

        public static (IComponentParser Parser, IComponentMatcher Matcher, IChangeDetector Detector) CreateServices()
        {
            try
            {
                System.Diagnostics.Debug.WriteLine("Starting comparison service creation...");

                System.Diagnostics.Debug.WriteLine("Creating strategies...");
                var strategies = CreateStrategies();

                System.Diagnostics.Debug.WriteLine("Creating ComponentParser...");
                var parser = new ComponentParser();

                System.Diagnostics.Debug.WriteLine("Creating ComponentMatcher...");
                var matcher = new ComponentMatcher(strategies);

                System.Diagnostics.Debug.WriteLine("Creating ChangeDetector...");
                var detector = new ChangeDetector(strategies);

                System.Diagnostics.Debug.WriteLine("All services created successfully");

                return (parser, matcher, detector);
            }
            catch (Exception ex)
            {
                System.Diagnostics.Debug.WriteLine($"Critical error creating comparison services: {ex}");
                throw new Exception("Failed to create comparison services", ex);
            }
        }
    }
}