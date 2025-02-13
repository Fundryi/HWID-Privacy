using System;
using System.Management;
using System.Diagnostics;
using System.Collections.Generic;
using System.Threading.Tasks;
using System.Linq;
using System.Text;
using System.Runtime.InteropServices;
using HWIDChecker.Services.Win32;
using static HWIDChecker.Services.Win32.SetupApi;

namespace HWIDChecker.Services
{
    public class SystemCleaningService
    {
        public event Action<string> OnStatusUpdate;
        public event Action<string, string> OnError;

        private record ProcessResult(string StdOut, string StdErr);
        public struct DeviceDetail
        {
            public string Name { get; }
            public string Description { get; }
            public string HardwareId { get; }
            public string Class { get; }
            public SP_DEVINFO_DATA DeviceInfoData;

            public DeviceDetail(string name, string description, string hardwareId, string deviceClass, SP_DEVINFO_DATA deviceInfoData)
            {
                Name = name;
                Description = description;
                HardwareId = hardwareId;
                Class = deviceClass;
                DeviceInfoData = deviceInfoData;
            }
        }

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

        public async Task CleanLogsAsync()
        {
            try
            {
                await CleanEventLogsAsync();
            }
            catch (Exception ex)
            {
                OnError?.Invoke("Cleaning Process", ex.Message);
                throw;
            }
        }

        private IntPtr _devicesHandle = IntPtr.Zero;

        public async Task<List<DeviceDetail>> ScanForGhostDevicesAsync()
        {
            var devices = new List<DeviceDetail>();
            var setupClass = Guid.Empty;
            
            // Store the device info set handle as a class field
            _devicesHandle = SetupDiGetClassDevs(ref setupClass, IntPtr.Zero, IntPtr.Zero, (uint)DiGetClassFlags.DIGCF_ALLCLASSES);

            if (_devicesHandle == IntPtr.Zero || _devicesHandle.ToInt64() == -1)
            {
                throw new Exception("Failed to get device list");
            }

            try
            {
                uint deviceIndex = 0;
                var deviceInfoData = new SP_DEVINFO_DATA { cbSize = (uint)Marshal.SizeOf(typeof(SP_DEVINFO_DATA)) };

                while (SetupDiEnumDeviceInfo(_devicesHandle, deviceIndex, ref deviceInfoData))
                {
                    var properties = new Dictionary<SetupDiGetDeviceRegistryPropertyEnum, string>();
                    var propertyArray = new[]
                    {
                        SetupDiGetDeviceRegistryPropertyEnum.SPDRP_FRIENDLYNAME,
                        SetupDiGetDeviceRegistryPropertyEnum.SPDRP_DEVICEDESC,
                        SetupDiGetDeviceRegistryPropertyEnum.SPDRP_HARDWAREID,
                        SetupDiGetDeviceRegistryPropertyEnum.SPDRP_CLASS,
                        SetupDiGetDeviceRegistryPropertyEnum.SPDRP_INSTALL_STATE
                    };

                    foreach (var prop in propertyArray)
                    {
                        var propBuffer = new byte[1024];
                        if (SetupDiGetDeviceRegistryProperty(_devicesHandle, ref deviceInfoData, (uint)prop,
                            out uint propType, propBuffer, (uint)propBuffer.Length, out uint requiredSize))
                        {
                            if (prop == SetupDiGetDeviceRegistryPropertyEnum.SPDRP_INSTALL_STATE)
                            {
                                properties[prop] = (requiredSize != 0).ToString();
                            }
                            else if (requiredSize > 0)
                            {
                                properties[prop] = Encoding.Unicode.GetString(propBuffer, 0, (int)requiredSize).Trim('\0');
                            }
                        }
                    }

                    // Check if device is present
                    bool isGhostDevice = true; // Assume it's a ghost device by default
                    foreach (var property in properties)
                    {
                        if (property.Key == SetupDiGetDeviceRegistryPropertyEnum.SPDRP_INSTALL_STATE)
                        {
                            isGhostDevice = property.Value == "False";
                            break;
                        }
                    }

                    if (isGhostDevice)
                    {
                        // Create an exact copy of the device info data for when we remove it
                        var deviceInfoCopy = new SP_DEVINFO_DATA
                        {
                            cbSize = deviceInfoData.cbSize,
                            classGuid = deviceInfoData.classGuid,
                            devInst = deviceInfoData.devInst,
                            reserved = deviceInfoData.reserved
                        };

                        var deviceName = "True"; // Match old script's behavior
                        var deviceDesc = properties.GetValueOrDefault(SetupDiGetDeviceRegistryPropertyEnum.SPDRP_DEVICEDESC) ??
                                       properties.GetValueOrDefault(SetupDiGetDeviceRegistryPropertyEnum.SPDRP_FRIENDLYNAME) ??
                                       "Unknown Device";

                        var hardwareIds = new List<string>();
                        if (properties.TryGetValue(SetupDiGetDeviceRegistryPropertyEnum.SPDRP_HARDWAREID, out var hwId))
                        {
                            hardwareIds.AddRange(hwId.Split('\0', StringSplitOptions.RemoveEmptyEntries));
                        }

                        var deviceClass = properties.GetValueOrDefault(SetupDiGetDeviceRegistryPropertyEnum.SPDRP_CLASS) ?? "";

                        devices.Add(new DeviceDetail(
                            deviceName,
                            deviceDesc,
                            string.Join("", hardwareIds),
                            deviceClass,
                            deviceInfoCopy));
                    }

                    deviceIndex++;
                }

                return devices;
            }
            catch
            {
                if (_devicesHandle != IntPtr.Zero && _devicesHandle.ToInt64() != -1)
                {
                    SetupDiDestroyDeviceInfoList(_devicesHandle);
                    _devicesHandle = IntPtr.Zero;
                }
                throw;
            }
        }

        public async Task RemoveGhostDevicesAsync(List<DeviceDetail> devices)
        {
            if (devices == null || devices.Count == 0) return;

            try
            {
                // Make sure we have a valid handle from scanning
                if (_devicesHandle == IntPtr.Zero || _devicesHandle.ToInt64() == -1)
                {
                    throw new Exception("Invalid device list handle. Please scan for devices first.");
                }

                OnStatusUpdate?.Invoke($"\r\nAttempting to remove {devices.Count} ghost device(s)...\r\n");
                int removedCount = 0;

                foreach (var device in devices)
                {
                    try
                    {
                        OnStatusUpdate?.Invoke($"Removing device: {device.Description} ({device.Class})");

                        // Use the exact same device info data we stored during scanning
                        var devInfoData = device.DeviceInfoData;
                        
                        // Attempt to remove the device directly
                        if (SetupDiRemoveDevice(_devicesHandle, ref devInfoData))
                        {
                            removedCount++;
                            OnStatusUpdate?.Invoke($"Successfully removed: {device.Description}");
                        }
                        else
                        {
                            var error = Marshal.GetLastWin32Error();
                            OnStatusUpdate?.Invoke($"Failed to remove: {device.Description}. Error code: {error}");
                        }
                    }
                    catch (Exception ex)
                    {
                        OnStatusUpdate?.Invoke($"Error removing {device.Description}: {ex.Message}");
                    }
                }

                OnStatusUpdate?.Invoke($"\r\nTotal devices removed: {removedCount}");
                if (removedCount < devices.Count)
                {
                    OnStatusUpdate?.Invoke($"Failed to remove {devices.Count - removedCount} device(s)");
                }
            }
            finally
            {
                // Clean up the device info set after we're done with removal
                if (_devicesHandle != IntPtr.Zero && _devicesHandle.ToInt64() != -1)
                {
                    SetupDiDestroyDeviceInfoList(_devicesHandle);
                    _devicesHandle = IntPtr.Zero;
                }
            }
        }

        [DllImport("setupapi.dll", SetLastError = true)]
        private static extern bool SetupDiGetDeviceInstanceId(IntPtr DeviceInfoSet, ref SP_DEVINFO_DATA DeviceInfoData,
            byte[] DeviceInstanceId, uint DeviceInstanceIdSize, out uint RequiredSize);

        [DllImport("setupapi.dll", SetLastError = true, CharSet = CharSet.Auto)]
        private static extern bool SetupDiOpenDeviceInfo(IntPtr deviceInfoSet, string deviceInstanceId,
            IntPtr hwndParent, uint openFlags, ref SP_DEVINFO_DATA deviceInfoData);

        [DllImport("setupapi.dll", SetLastError = true)]
        private static extern bool SetupDiDestroyDeviceInfoList(IntPtr DeviceInfoSet);

        private readonly string[] StandardEventLogs = new[]
        {
            "Windows PowerShell",
            "System",
            "Security",
            "Application",
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
            "Microsoft-Windows-Application-Experience/Program-Telemetry"
        };

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
                            OnStatusUpdate?.Invoke($"Constructed log file path: {logPath}");

                            if (System.IO.File.Exists(logPath))
                            {
                                OnStatusUpdate?.Invoke("Log file exists. Attempting to take ownership and grant full control...");
                                var takeownResult = await RunProcessAsync("takeown.exe", $"/f \"{logPath}\"");
                                var icaclsResult = await RunProcessAsync("icacls.exe", $"\"{logPath}\" /grant:r \"{Environment.UserName}:(F)\"");
                            }
                            else
                            {
                                OnStatusUpdate?.Invoke("Log file not found at the expected location. Skipping this log.");
                            }
                        }

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