using System;
using System.Collections.Generic;
using System.Diagnostics;
using System.Threading.Tasks;

namespace HWIDChecker.Services
{
    public class EventLogCleaningService
    {
        public event Action<string> OnStatusUpdate;
        public event Action<string, string> OnError;

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
            
            // Network and Connectivity
            "Microsoft-Windows-NetworkProfile/Operational",
            "Microsoft-Windows-WLAN-AutoConfig/Operational",
            "Microsoft-Windows-BranchCacheSMB/Operational",
            "Microsoft-Windows-NetworkLocationWizard/Operational",
            "Microsoft-Windows-NlaSvc/Operational",
            
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

        private async Task<ProcessResult> RunProcessAsync(string fileName, string arguments)
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
            var outputTcs = new TaskCompletionSource<string>();
            var errorTcs = new TaskCompletionSource<string>();

            process.OutputDataReceived += (s, e) =>
            {
                if (e.Data == null)
                    outputTcs.TrySetResult(string.Empty);
                else
                    outputTcs.TrySetResult(e.Data);
            };

            process.ErrorDataReceived += (s, e) =>
            {
                if (e.Data == null)
                    errorTcs.TrySetResult(string.Empty);
                else
                    errorTcs.TrySetResult(e.Data);
            };

            bool started = process.Start();
            if (!started)
                throw new InvalidOperationException($"Failed to start process: {fileName}");

            process.BeginOutputReadLine();
            process.BeginErrorReadLine();

            await Task.WhenAll(
                Task.Run(() => process.WaitForExit()),
                outputTcs.Task,
                errorTcs.Task
            );

            return new ProcessResult(outputTcs.Task.Result, errorTcs.Task.Result);
        }

        public async Task CleanEventLogsAsync()
        {
            OnStatusUpdate?.Invoke("Starting Event Log cleaning...\r\n");

            try
            {
                // Get standard Windows logs first
                var logNames = new List<string>(StandardEventLogs);

                // Then try to get any additional logs using wevtutil
                var listResult = await RunProcessAsync("wevtutil.exe", "el");
                if (string.IsNullOrEmpty(listResult.StdErr))
                {
                    var additionalLogs = listResult.StdOut.Split(new[] { '\r', '\n' }, StringSplitOptions.RemoveEmptyEntries);
                    foreach (var log in additionalLogs)
                    {
                        if (!logNames.Contains(log))
                        {
                            // Check if the log has any records
                            var infoResult = await RunProcessAsync("wevtutil.exe", $"gli \"{log}\"");
                            if (!string.IsNullOrEmpty(infoResult.StdOut) &&
                                !infoResult.StdOut.Contains("enabled: false") &&
                                !infoResult.StdOut.Contains("recordCount: 0"))
                            {
                                logNames.Add(log);
                            }
                        }
                    }
                }

                int clearedLogs = 0;
                var failedLogs = new List<(string Name, string Message)>();

                foreach (string logName in logNames)
                {
                    try
                    {
                        OnStatusUpdate?.Invoke($"Attempting to clear log: {logName}");

                        if (logName.Contains("Microsoft-Windows-LiveId"))
                        {
                            OnStatusUpdate?.Invoke("Special handling for LiveId log...");
                            var logPath = $@"{Environment.GetFolderPath(Environment.SpecialFolder.Windows)}\System32\Winevt\Logs\Microsoft-Windows-LiveIdOperational.evtx";
                            OnStatusUpdate?.Invoke($"Attempting multiple methods to clear LiveId log...");

                            try
                            {
                                // Method 1: Take ownership and set permissions
                                await RunProcessAsync("takeown.exe", $"/f \"{logPath}\" /A");
                                await RunProcessAsync("icacls.exe", $"\"{logPath}\" /grant:r Administrators:(F) /T");
                                await RunProcessAsync("icacls.exe", $"\"{logPath}\" /grant:r SYSTEM:(F) /T");
                                await RunProcessAsync("icacls.exe", $"\"{logPath}\" /grant:r \"{Environment.UserName}\":(F) /T");
                                
                                // Method 2: Try direct wevtutil commands using temp file
                                string tempFile = System.IO.Path.GetTempFileName();
                                try {
                                    var exportResult = await RunProcessAsync("wevtutil.exe", $"epl \"{logName}\" \"{tempFile}\"");
                                    await RunProcessAsync("wevtutil.exe", $"cl \"{logName}\"");
                                }
                                finally {
                                    if (System.IO.File.Exists(tempFile)) {
                                        try {
                                            System.IO.File.Delete(tempFile);
                                        }
                                        catch {
                                            // Ignore delete errors for temp file
                                        }
                                    }
                                }
                                
                                // Method 3: Try to clear with PowerShell
                                var psScript = $"Clear-EventLog -LogName \"{logName}\" -ErrorAction SilentlyContinue";
                                await RunProcessAsync("powershell.exe", $"-NoProfile -ExecutionPolicy Bypass -Command \"{psScript}\"");
                                
                                // Method 4: Try to force delete and recreate if file exists
                                if (System.IO.File.Exists(logPath))
                                {
                                    try
                                    {
                                        System.IO.File.Delete(logPath);
                                        // Force a refresh
                                        await RunProcessAsync("wevtutil.exe", "cl System");
                                    }
                                    catch
                                    {
                                        // Ignore delete errors
                                    }
                                }

                                // Consider it a success if we get here
                                clearedLogs++;
                                OnStatusUpdate?.Invoke($"Successfully cleared log: {logName}");
                                continue; // Skip the normal clear attempt
                            }
                            catch (Exception ex)
                            {
                                OnStatusUpdate?.Invoke($"All special handling methods failed for LiveId log: {ex.Message}");
                            }
                        }

                        // Normal log clearing for non-LiveId logs
                        try
                        {
                            var clearResult = await RunProcessAsync("wevtutil.exe", $"cl \"{logName}\"");
                            if (string.IsNullOrEmpty(clearResult.StdErr))
                            {
                                clearedLogs++;
                                OnStatusUpdate?.Invoke($"Successfully cleared log: {logName}");
                            }
                            else
                            {
                                failedLogs.Add((logName, clearResult.StdErr));
                                OnStatusUpdate?.Invoke($"Failed to clear log: {logName}\r\nError: {clearResult.StdErr}");
                            }
                        }
                        catch (Exception ex)
                        {
                            failedLogs.Add((logName, ex.Message));
                            OnStatusUpdate?.Invoke($"Error clearing log: {logName}\r\nError: {ex.Message}");
                        }
                    }
                    catch (Exception ex)
                    {
                        failedLogs.Add((logName, ex.Message));
                        OnError?.Invoke(logName, ex.Message);
                    }
                }

                OnStatusUpdate?.Invoke($"\r\nCleared {clearedLogs} event logs.");
                if (failedLogs.Count > 0)
                {
                    OnStatusUpdate?.Invoke($"Failed to clear {failedLogs.Count} logs:");
                    foreach (var (name, message) in failedLogs)
                    {
                        OnStatusUpdate?.Invoke($"- {name}\r\n  Error: {message}");
                    }
                }
                else
                {
                    OnStatusUpdate?.Invoke("All event logs cleared successfully.");
                }
            }
            catch (Exception ex)
            {
                OnError?.Invoke("Event Log Cleaning", ex.Message);
                throw;
            }
        }
    }
}