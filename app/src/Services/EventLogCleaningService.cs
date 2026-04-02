using System;
using System.Collections.Concurrent;
using System.Collections.Generic;
using System.Diagnostics;
using System.Linq;
using System.Threading;
using System.Threading.Tasks;
using HWIDChecker.Services.Win32;

namespace HWIDChecker.Services
{
    public class EventLogCleaningService
    {
        private const int ProcessTimeoutMs = 15000;
        private const int AdditionalDiscoveryMaxDegreeOfParallelism = 12;
        private const int LogCleaningMaxDegreeOfParallelism = 10;
        private const int DiscoveryProgressReportInterval = 50;

        public event Action<string> OnStatusUpdate;
        public event Action<string, string> OnError;

        // Logs held open by kernel drivers or active OS services — cannot be cleared
        // while Windows is running. Attempting wastes time on fallback methods.
        private static readonly HashSet<string> UnclearableLogs = new(StringComparer.OrdinalIgnoreCase)
        {
            // Kernel/driver logs (held by running drivers)
            "Microsoft-Windows-Kernel-PnP/Device Management",
            "Microsoft-Windows-Kernel-Cache/Operational",
            "Microsoft-Windows-StorageVolume/Operational",
            "Microsoft-Windows-Storage-Partition/Diagnostic",
            "Microsoft-Windows-FilterManager/Operational",
            "Microsoft-Windows-DriverFrameworks-UserMode/Diagnostic",
            "Microsoft-Windows-Hyper-V-Drivers/Operational",
            "Microsoft-Windows-USB-USBHUB/Operational",
            "Microsoft-Windows-USB-USBPORT/Operational",
            "Microsoft-Windows-Hardware-Events/Operational",
            // Active network service logs (held by running services)
            "Microsoft-Windows-NetworkSecurity/Operational",
            "Microsoft-Windows-NetworkConnectivityStatus/Operational",
            "Microsoft-Windows-NetCore/Operational",
            "Microsoft-Windows-WLAN/Diagnostic",
            "Microsoft-Windows-WWAN-MM-Events/Operational",
            "Microsoft-Windows-WWAN-UI-Events/Operational",
            // Security audit logs (require kernel-level audit policy, not just process privileges)
            "Microsoft-Windows-Security-Auditing/Operational",
            "Microsoft-Windows-Security-SPP/Operational",
            // Other in-use service logs
            "Microsoft-Windows-LiveId/Operational",
            "Microsoft-Windows-PCW/Operational",
            "Microsoft-Windows-DeviceInstall/Operational",
            "Microsoft-Windows-DeviceAssociation/Operational",
            "Microsoft-Windows-Diagnosis-Schedule/Operational",
        };

        private static string NormalizeLogName(string logName)
        {
            return (logName ?? string.Empty).Trim();
        }

        private static List<string> BuildUniqueLogList(IEnumerable<string> source, out int duplicatesRemoved)
        {
            var unique = new List<string>();
            var seen = new HashSet<string>(StringComparer.OrdinalIgnoreCase);
            duplicatesRemoved = 0;

            foreach (var item in source)
            {
                var logName = NormalizeLogName(item);
                if (string.IsNullOrEmpty(logName))
                {
                    continue;
                }

                if (seen.Add(logName))
                {
                    unique.Add(logName);
                }
                else
                {
                    duplicatesRemoved++;
                }
            }

            return unique;
        }

        private async Task<bool> ClearLogWithAdvancedMethodsAsync(string logName, CancellationToken cancellationToken)
        {
            try
            {
                // Debug/Analytic channels must be disabled before clearing, then re-enabled
                var channelType = EventLogApi.GetChannelType(logName);
                bool isDebugAnalytic = channelType == EventLogApi.EvtChannelTypeAnalytic ||
                                       channelType == EventLogApi.EvtChannelTypeDebug;

                if (isDebugAnalytic)
                {
                    if (EventLogApi.SetChannelEnabled(logName, false))
                    {
                        try
                        {
                            var result = await RunProcessAsync("wevtutil.exe", $"cl \"{logName}\"", cancellationToken: cancellationToken);
                            if (string.IsNullOrEmpty(result.StdErr))
                            {
                                OnStatusUpdate?.Invoke($"Cleared: {logName}");
                                return true;
                            }
                            return false;
                        }
                        finally
                        {
                            EventLogApi.SetChannelEnabled(logName, true);
                        }
                    }
                    return false;
                }

                // Method 1: wevtutil process — most reliable, handles all privilege requirements
                var clearResult = await RunProcessAsync("wevtutil.exe", $"cl \"{logName}\"", cancellationToken: cancellationToken);
                if (string.IsNullOrEmpty(clearResult.StdErr))
                {
                    OnStatusUpdate?.Invoke($"Cleared: {logName}");
                    return true;
                }

                // Construct log file path (handle both / and - in log names)
                string logFileName = logName.Replace("/", "%4").Replace("-", "_").Replace(" ", "_") + ".evtx";
                string logPath = $@"{Environment.GetFolderPath(Environment.SpecialFolder.Windows)}\System32\Winevt\Logs\{logFileName}";

                // Method 3: Fix permissions then retry via process
                if (System.IO.File.Exists(logPath))
                {
                    await RunProcessAsync("takeown.exe", $"/f \"{logPath}\" /A", cancellationToken: cancellationToken);
                    await RunProcessAsync("icacls.exe", $"\"{logPath}\" /grant:r Administrators:(F) /T", cancellationToken: cancellationToken);
                    await RunProcessAsync("icacls.exe", $"\"{logPath}\" /grant:r SYSTEM:(F) /T", cancellationToken: cancellationToken);
                    await RunProcessAsync("icacls.exe", $"\"{logPath}\" /grant:r \"{Environment.UserName}\":(F) /T", cancellationToken: cancellationToken);

                    var retryResult = await RunProcessAsync("wevtutil.exe", $"cl \"{logName}\"", cancellationToken: cancellationToken);
                    if (string.IsNullOrEmpty(retryResult.StdErr))
                    {
                        OnStatusUpdate?.Invoke($"Cleared: {logName}");
                        return true;
                    }
                }

                // Method 4: Export then clear (forces the log handle to release)
                string tempFile = System.IO.Path.GetTempFileName();
                try
                {
                    await RunProcessAsync("wevtutil.exe", $"epl \"{logName}\" \"{tempFile}\"", cancellationToken: cancellationToken);
                    var eplClearResult = await RunProcessAsync("wevtutil.exe", $"cl \"{logName}\"", cancellationToken: cancellationToken);
                    if (string.IsNullOrEmpty(eplClearResult.StdErr))
                    {
                        OnStatusUpdate?.Invoke($"Cleared: {logName}");
                        return true;
                    }
                }
                finally
                {
                    if (System.IO.File.Exists(tempFile))
                    {
                        try { System.IO.File.Delete(tempFile); }
                        catch { /* Ignore temp file cleanup errors */ }
                    }
                }

                // Method 5: Force delete the .evtx file (last resort)
                if (System.IO.File.Exists(logPath))
                {
                    try
                    {
                        System.IO.File.Delete(logPath);
                        OnStatusUpdate?.Invoke($"Cleared: {logName}");
                        return true;
                    }
                    catch { /* Ignore delete errors */ }
                }

                return false;
            }
            catch (OperationCanceledException)
            {
                throw;
            }
            catch (Exception ex)
            {
                OnStatusUpdate?.Invoke($"Failed: {logName} - {ex.Message}");
                return false;
            }
        }

        private readonly string[] StandardEventLogs = new[]
        {
            // Standard Windows Logs
            "Windows PowerShell",
            "System",
            "Security",
            "Application",
            "PowerShellCore/Operational",
            
            // Storage and Device Related Logs
            "Microsoft-Windows-Storage-Storport/Operational",
            "Microsoft-Windows-Storage-ClassPnP/Operational",
            "Microsoft-Windows-Storage-Partition/Diagnostic",
            "Microsoft-Windows-StorageSpaces-Driver/Operational",
            "Microsoft-Windows-StorageVolume/Operational",
            "Microsoft-Windows-Ntfs/Operational",
            "Microsoft-Windows-VolumeSnapshot-Driver/Operational",
            
            // Device Management Logs
            "Microsoft-Windows-DeviceSetupManager/Admin",
            "Microsoft-Windows-DeviceSetupManager/Operational",
            "Microsoft-Windows-Kernel-PnP/Device Management",
            "Microsoft-Windows-Kernel-PnP/Configuration",
            "Microsoft-Windows-UserPnp/DeviceInstall",
            "Microsoft-Windows-DeviceManagement-Enterprise-Diagnostics-Provider/Admin",
            
            // System Configuration and State
            "Microsoft-Windows-StateRepository/Operational",
            "Microsoft-Windows-CodeIntegrity/Operational",
            "Microsoft-Windows-Kernel-ShimEngine/Operational",
            "Microsoft-Windows-Kernel-EventTracing/Admin",
            "Microsoft-Windows-GroupPolicy/Operational",
            "Microsoft-Windows-Known Folders API Service",
            
            // Hardware Monitoring and Diagnostics
            "Microsoft-Windows-DriverFrameworks-UserMode/Operational",
            "Microsoft-Windows-Hardware-Events/Operational",
            "Microsoft-Windows-DeviceGuard/Operational",
            "Microsoft-Windows-DNS-Client/Operational",
            "Microsoft-Windows-Hyper-V-Drivers/Operational",
            "Microsoft-Windows-Resource-Exhaustion-Detector/Operational",
            
            // Authentication and Security
            "Microsoft-Windows-Authentication/AuthenticationPolicyFailures-DomainController",
            "Microsoft-Windows-Authentication/ProtectedUser-Client",
            "Microsoft-Windows-Security-SPP/Operational",
            "Microsoft-Windows-Security-Auditing/Operational",
            
            // Network, Connectivity and Hardware History
            "Microsoft-Windows-NetworkProfile/Operational",
            "Microsoft-Windows-WLAN-AutoConfig/Operational",
            "Microsoft-Windows-BranchCacheSMB/Operational",
            "Microsoft-Windows-NetworkLocationWizard/Operational",
            "Microsoft-Windows-NlaSvc/Operational",
            "Microsoft-Windows-Dhcp-Client/Admin",
            "Microsoft-Windows-Dhcp-Client/Operational",
            "Microsoft-Windows-DHCPv6-Client/Operational",
            "Microsoft-Windows-TCPIP/Operational",
            "Microsoft-Windows-WLAN-AutoConfig/Diagnostic",
            "Microsoft-Windows-Iphlpsvc/Operational",
            "Microsoft-Windows-NetworkConnectivityStatus/Operational",
            "Microsoft-Windows-NetCore/Operational",
            
            // Wireless and Bluetooth Device History
            "Microsoft-Windows-Bluetooth-BthLEPrepairing/Operational",
            "Microsoft-Windows-Bluetooth-MTPEnum/Operational",
            "Microsoft-Windows-WLAN/Diagnostic",
            "Microsoft-Windows-WWAN-SVC-Events/Operational",
            "Microsoft-Windows-WWAN-UI-Events/Operational",
            "Microsoft-Windows-WWAN-MM-Events/Operational",
            
            // Additional Device and Driver History
            "Microsoft-Windows-DeviceAssociation/Operational",
            "Microsoft-Windows-DeviceInstall/Operational",
            "Microsoft-Windows-DriverFrameworks-UserMode/Diagnostic",
            "Microsoft-Windows-PCW/Operational",
            "Microsoft-Windows-EapHost/Operational",
            "Microsoft-Windows-FilterManager/Operational",
            
            // Network Security and Authentication
            "Microsoft-Windows-Dhcpv6-Client/Admin",
            "Microsoft-Windows-WebAuthN/Operational",
            "Microsoft-Windows-WFP/Operational",
            "Microsoft-Windows-Windows Firewall With Advanced Security/Firewall",
            "Microsoft-Windows-NetworkSecurity/Operational",
            
            // Core System Services
            "Microsoft-Windows-WMI-Activity/Operational",
            "Microsoft-Windows-Time-Service/Operational",
            "Microsoft-Windows-Store/Operational",
            "Microsoft-Windows-Shell-Core/Operational",
            "Microsoft-Windows-Security-Mitigations/KernelMode",
            "Microsoft-Windows-PushNotification-Platform/Operational",
            "Microsoft-Windows-PowerShell/Operational",
            "Microsoft-Windows-LiveId/Operational",
            "Microsoft-Windows-Kernel-Cache/Operational",
            "Microsoft-Windows-Diagnosis-PCW/Operational",
            "Microsoft-Windows-AppModel-Runtime/Admin",
            "Microsoft-Windows-Application-Experience/Program-Telemetry",
            "Microsoft-Windows-AppxPackaging/Operational",
            
            // System Diagnostics and Troubleshooting
            "Microsoft-Windows-Diagnostics-Performance/Operational",
            "Microsoft-Windows-Diagnosis-Scripted/Operational",
            "Microsoft-Windows-Diagnosis-Schedule/Operational",
            "Microsoft-Windows-USB-USBHUB/Operational",
            "Microsoft-Windows-USB-USBPORT/Operational",
            "Microsoft-Windows-Winlogon/Operational",
            "Microsoft-Windows-UAC/Operational"
        };

        private record ProcessResult(string StdOut, string StdErr);

        private async Task<ProcessResult> RunProcessAsync(
            string fileName,
            string arguments,
            int timeoutMs = ProcessTimeoutMs,
            CancellationToken cancellationToken = default)
        {
            var startInfo = new ProcessStartInfo
            {
                FileName = fileName,
                Arguments = arguments,
                UseShellExecute = false,
                RedirectStandardOutput = true,
                RedirectStandardError = true,
                CreateNoWindow = true
            };

            using var process = new Process { StartInfo = startInfo };

            bool started = process.Start();
            if (!started)
            {
                throw new InvalidOperationException($"Failed to start process: {fileName}");
            }

            var stdOutTask = process.StandardOutput.ReadToEndAsync();
            var stdErrTask = process.StandardError.ReadToEndAsync();

            try
            {
                using var linkedCts = CancellationTokenSource.CreateLinkedTokenSource(cancellationToken);
                linkedCts.CancelAfter(timeoutMs);
                await process.WaitForExitAsync(linkedCts.Token);
            }
            catch (OperationCanceledException) when (cancellationToken.IsCancellationRequested)
            {
                try
                {
                    if (!process.HasExited)
                    {
                        process.Kill(entireProcessTree: true);
                    }
                }
                catch
                {
                }

                throw new OperationCanceledException($"Process canceled: {fileName} {arguments}", cancellationToken);
            }
            catch (OperationCanceledException)
            {
                try
                {
                    if (!process.HasExited)
                    {
                        process.Kill(entireProcessTree: true);
                    }
                }
                catch
                {
                }

                throw new TimeoutException($"Process timed out after {timeoutMs}ms: {fileName} {arguments}");
            }

            var stdOut = await stdOutTask;
            var stdErr = await stdErrTask;

            if (process.ExitCode != 0 && string.IsNullOrWhiteSpace(stdErr))
            {
                stdErr = $"Process exited with code {process.ExitCode}.";
            }

            return new ProcessResult(stdOut ?? string.Empty, stdErr ?? string.Empty);
        }

        public async Task CleanEventLogsAsync(CancellationToken cancellationToken = default)
        {
            try
            {
                cancellationToken.ThrowIfCancellationRequested();

                // Enable SeSecurityPrivilege for clearing security-related logs
                if (EventLogApi.EnablePrivilege("SeSecurityPrivilege"))
                    OnStatusUpdate?.Invoke("Elevated: SeSecurityPrivilege enabled.");
                if (EventLogApi.EnablePrivilege("SeBackupPrivilege"))
                    OnStatusUpdate?.Invoke("Elevated: SeBackupPrivilege enabled.");

                OnStatusUpdate?.Invoke("Collecting standard event log channels...");
                var logNames = BuildUniqueLogList(StandardEventLogs, out var standardDuplicatesRemoved);
                var knownLogNames = new HashSet<string>(logNames, StringComparer.OrdinalIgnoreCase);
                var processedLogNames = new HashSet<string>(StringComparer.OrdinalIgnoreCase);

                if (standardDuplicatesRemoved > 0)
                {
                    OnStatusUpdate?.Invoke($"Skipped {standardDuplicatesRemoved} duplicate standard channels.");
                }
                OnStatusUpdate?.Invoke($"Collected {logNames.Count} standard logs to process.");

                int standardLogsCount = logNames.Count;
                int additionalLogsCount = 0;
                int attemptedLogs = 0;
                int clearedLogs = 0;
                int skippedNotFound = 0;
                int skippedDisabled = 0;
                int skippedDuplicate = 0;
                int skippedUnclearable = 0;
                var failedLogs = new ConcurrentBag<(string Name, string Message)>();

                async Task ProcessLogBatchAsync(IEnumerable<string> batch, bool skipExistenceProbe, CancellationToken token)
                {
                    // Pre-filter before parallel execution (processedLogNames is not thread-safe)
                    var filteredBatch = new List<string>();
                    foreach (var batchLogName in batch)
                    {
                        var logName = NormalizeLogName(batchLogName);
                        if (string.IsNullOrEmpty(logName))
                            continue;

                        if (!processedLogNames.Add(logName))
                        {
                            Interlocked.Increment(ref skippedDuplicate);
                            OnStatusUpdate?.Invoke($"Skipped: {logName} (duplicate)");
                            continue;
                        }

                        if (UnclearableLogs.Contains(logName))
                        {
                            Interlocked.Increment(ref skippedUnclearable);
                            continue;
                        }

                        filteredBatch.Add(logName);
                    }

                    int batchTotal = filteredBatch.Count;
                    int batchProcessed = 0;

                    await Parallel.ForEachAsync(
                        filteredBatch,
                        new ParallelOptions
                        {
                            MaxDegreeOfParallelism = LogCleaningMaxDegreeOfParallelism,
                            CancellationToken = token
                        },
                        async (logName, loopToken) =>
                        {
                            try
                            {
                                if (!skipExistenceProbe)
                                {
                                    var enabled = EventLogApi.IsChannelEnabled(logName);
                                    if (enabled == null)
                                    {
                                        Interlocked.Increment(ref skippedNotFound);
                                        return;
                                    }
                                    if (enabled == false)
                                    {
                                        Interlocked.Increment(ref skippedDisabled);
                                        return;
                                    }
                                }

                                Interlocked.Increment(ref attemptedLogs);
                                var current = Interlocked.Increment(ref batchProcessed);
                                OnStatusUpdate?.Invoke($"Clearing log {current}/{batchTotal}: {logName}");

                                if (await ClearLogWithAdvancedMethodsAsync(logName, loopToken))
                                {
                                    Interlocked.Increment(ref clearedLogs);
                                }
                                else
                                {
                                    failedLogs.Add((logName, "Failed to clear log after trying all available methods"));
                                }
                            }
                            catch (OperationCanceledException)
                            {
                                throw;
                            }
                            catch (Exception ex)
                            {
                                failedLogs.Add((logName, ex.Message));
                                OnError?.Invoke(logName, ex.Message);
                            }
                        });
                }

                // Run standard cleaning and additional log discovery in parallel.
                // Discovery is pure read (wevtutil gli probes) and doesn't depend on standard cleaning.
                var standardCleaningTask = ProcessLogBatchAsync(logNames, skipExistenceProbe: true, cancellationToken);
                OnStatusUpdate?.Invoke("Discovering additional event log channels in background...");
                var discoveryTask = TryCollectAdditionalLogsAsync(knownLogNames, cancellationToken);

                // Wait for both to complete before processing additional logs.
                await standardCleaningTask;
                var additionalLogs = await discoveryTask;

                if (additionalLogs.Count > 0)
                {
                    additionalLogsCount = additionalLogs.Count;
                    OnStatusUpdate?.Invoke($"Collected {additionalLogs.Count} additional logs to process.");
                    await ProcessLogBatchAsync(additionalLogs, skipExistenceProbe: false, cancellationToken);
                }
                else
                {
                    OnStatusUpdate?.Invoke("No additional event logs to process.");
                }

                var failedLogsList = failedLogs.ToList();
                string summary = $"Summary: {clearedLogs} logs cleared";
                if (failedLogsList.Count > 0)
                {
                    summary += $", {failedLogsList.Count} failed";
                    OnStatusUpdate?.Invoke(summary);
                    foreach (var (name, message) in failedLogsList)
                    {
                        OnStatusUpdate?.Invoke($"Failed: {name} - {message}");
                    }
                }
                else
                {
                    OnStatusUpdate?.Invoke($"{summary} successfully");
                }

                var totalCollected = standardLogsCount + additionalLogsCount;
                OnStatusUpdate?.Invoke(string.Empty);
                OnStatusUpdate?.Invoke("========== CLEAN LOGS OVERVIEW ==========");
                OnStatusUpdate?.Invoke($"Collected logs (standard)  : {standardLogsCount}");
                OnStatusUpdate?.Invoke($"Collected logs (additional): {additionalLogsCount}");
                OnStatusUpdate?.Invoke($"Collected logs (total)     : {totalCollected}");
                OnStatusUpdate?.Invoke($"Logs attempted             : {attemptedLogs}");
                OnStatusUpdate?.Invoke($"Logs cleared               : {clearedLogs}");
                OnStatusUpdate?.Invoke($"Skipped (not found)        : {skippedNotFound}");
                OnStatusUpdate?.Invoke($"Skipped (disabled)         : {skippedDisabled}");
                OnStatusUpdate?.Invoke($"Skipped (duplicate)        : {skippedDuplicate}");
                OnStatusUpdate?.Invoke($"Skipped (OS-locked)        : {skippedUnclearable}");
                OnStatusUpdate?.Invoke($"Failed                     : {failedLogsList.Count}");
                OnStatusUpdate?.Invoke("=========================================");
            }
            catch (OperationCanceledException)
            {
                OnStatusUpdate?.Invoke(string.Empty);
                OnStatusUpdate?.Invoke("Log cleaning canceled by user.");
                throw;
            }
            catch (Exception ex)
            {
                OnError?.Invoke("Event Log Cleaning", ex.Message);
                throw;
            }
        }

        private Task<List<string>> TryCollectAdditionalLogsAsync(HashSet<string> knownLogNames, CancellationToken cancellationToken)
        {
            return Task.Run(() =>
            {
                var additionalLogs = new List<string>();

                List<string> discoveredLogs;
                try
                {
                    discoveredLogs = EventLogApi.EnumerateChannels();
                }
                catch (Exception ex)
                {
                    OnStatusUpdate?.Invoke($"Skipped additional log discovery: {ex.Message}");
                    return additionalLogs;
                }

                var candidateLogs = new List<string>();
                int duplicateDiscoveredChannels = 0;
                var discoveredUnique = new HashSet<string>(StringComparer.OrdinalIgnoreCase);
                foreach (var rawLog in discoveredLogs)
                {
                    cancellationToken.ThrowIfCancellationRequested();

                    var log = NormalizeLogName(rawLog);
                    if (string.IsNullOrEmpty(log))
                        continue;

                    if (knownLogNames.Contains(log))
                    {
                        duplicateDiscoveredChannels++;
                        continue;
                    }

                    if (!discoveredUnique.Add(log))
                    {
                        duplicateDiscoveredChannels++;
                        continue;
                    }

                    candidateLogs.Add(log);
                }

                if (duplicateDiscoveredChannels > 0)
                {
                    OnStatusUpdate?.Invoke($"Skipped {duplicateDiscoveredChannels} channels already known from standard/discovered sets.");
                }

                if (candidateLogs.Count == 0)
                {
                    return additionalLogs;
                }

                OnStatusUpdate?.Invoke($"Probing {candidateLogs.Count} additional channels...");

                var additionalLogBag = new ConcurrentBag<string>();
                int processedCount = 0;

                Parallel.ForEach(
                    candidateLogs,
                    new ParallelOptions
                    {
                        MaxDegreeOfParallelism = AdditionalDiscoveryMaxDegreeOfParallelism,
                        CancellationToken = cancellationToken
                    },
                    log =>
                    {
                        try
                        {
                            // Check enabled state via channel config API
                            var enabled = EventLogApi.IsChannelEnabled(log);
                            if (enabled == false)
                                return;

                            // Include all enabled channels — EvtClearLog returns instantly on empty logs,
                            // so probing record count adds overhead without value.
                            additionalLogBag.Add(log);
                        }
                        catch (OperationCanceledException)
                        {
                            throw;
                        }
                        catch
                        {
                            // Skip channels that fail during metadata probe
                        }
                        finally
                        {
                            var current = Interlocked.Increment(ref processedCount);
                            if (current % DiscoveryProgressReportInterval == 0 || current == candidateLogs.Count)
                            {
                                OnStatusUpdate?.Invoke($"Discovering additional logs... {current}/{candidateLogs.Count}");
                            }
                        }
                    });

                foreach (var log in additionalLogBag.OrderBy(log => log, StringComparer.OrdinalIgnoreCase))
                {
                    if (knownLogNames.Add(log))
                    {
                        additionalLogs.Add(log);
                    }
                }

                return additionalLogs;
            }, cancellationToken);
        }
    }
}
