using System;
using System.Collections.Generic;
using System.Text;
using System.Threading.Tasks;
using System.Linq;
using HWIDChecker.Services;

namespace HWIDChecker.Hardware
{
    public class HardwareInfoManager
    {
        private readonly List<IHardwareInfo> hardwareInfoProviders;
        private readonly TextFormattingService textFormatter;

        public HardwareInfoManager()
        {
            textFormatter = new TextFormattingService();
            hardwareInfoProviders = InitializeProviders();
        }

        private List<IHardwareInfo> InitializeProviders()
        {
            return new List<IHardwareInfo>
            {
                new DiskDriveInfo(textFormatter),
                new MotherboardInfo(textFormatter),
                new BiosInfo(textFormatter),
                new SystemInfo(),
                new RamInfo(textFormatter),
                new CpuInfo(),
                new TpmInfo(),
                new UsbInfo(textFormatter),
                new GpuInfo(),
                new MonitorInfo(textFormatter),
                new NetworkInfo(textFormatter),
                new ArpInfo(textFormatter)
            };
        }

        public int GetProviderCount() => hardwareInfoProviders.Count;

        public async Task<string> GetAllHardwareInfo(IProgress<(int index, string content)> progress = null)
        {
            try
            {
                System.Diagnostics.Debug.WriteLine("Starting GetAllHardwareInfo...");
                var sb = new StringBuilder();
                sb.Append(textFormatter.FormatHeader("Comprehensive HWID Checker"));

                System.Diagnostics.Debug.WriteLine($"Provider count: {hardwareInfoProviders.Count}");

                // Create array to store results in order
                var orderedResults = new string[hardwareInfoProviders.Count];

                // Start all tasks in parallel with index
                var tasks = hardwareInfoProviders.Select(async (provider, index) =>
                {
                    try
                    {
                        System.Diagnostics.Debug.WriteLine($"Getting info from provider: {provider.SectionTitle}");
                        var information = await Task.Run(() => provider.GetInformation());
                        var formattedSection = textFormatter.FormatSection(provider.SectionTitle, information);

                        System.Diagnostics.Debug.WriteLine($"Successfully got info from {provider.SectionTitle}");

                        // Store in ordered array and report progress
                        orderedResults[index] = formattedSection;
                        progress?.Report((index, formattedSection));

                        return (index, content: formattedSection);
                    }
                    catch (Exception ex)
                    {
                        System.Diagnostics.Debug.WriteLine($"Error in provider {provider.SectionTitle}: {ex}");
                        var errorMessage = $"Error retrieving {provider.SectionTitle} information: {ex.Message}";
                        var formattedError = textFormatter.FormatSection(provider.SectionTitle, errorMessage);

                        // Store error in ordered array and report
                        orderedResults[index] = formattedError;
                        progress?.Report((index, formattedError));

                        return (index, content: formattedError);
                    }
                }).ToList();

                // Wait for all tasks to complete
                System.Diagnostics.Debug.WriteLine("Waiting for all tasks to complete...");
                await Task.WhenAll(tasks);
                System.Diagnostics.Debug.WriteLine("All tasks completed");

                // Append results in original order
                foreach (var result in orderedResults)
                {
                    sb.Append(result);
                }

                var finalResult = sb.ToString();
                System.Diagnostics.Debug.WriteLine($"GetAllHardwareInfo completed. Result length: {finalResult.Length}");
                return finalResult;
            }
            catch (Exception ex)
            {
                System.Diagnostics.Debug.WriteLine($"Critical error in GetAllHardwareInfo: {ex}");
                throw;
            }
        }

        public string GetSpecificHardwareInfo(string sectionTitle)
        {
            var provider = hardwareInfoProviders.Find(p => p.SectionTitle.Equals(sectionTitle, StringComparison.OrdinalIgnoreCase));

            if (provider == null)
                return $"No provider found for section: {sectionTitle}";

            try
            {
                var information = provider.GetInformation();
                return textFormatter.FormatSection(provider.SectionTitle, information);
            }
            catch (Exception ex)
            {
                return textFormatter.FormatSection(provider.SectionTitle,
                    $"Error retrieving information: {ex.Message}");
            }
        }

        public List<string> GetAvailableSections()
        {
            return hardwareInfoProviders.ConvertAll(p => p.SectionTitle);
        }
    }
}