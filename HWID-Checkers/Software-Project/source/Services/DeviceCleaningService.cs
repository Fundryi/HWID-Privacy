using System;
using System.Collections.Generic;
using System.Runtime.InteropServices;
using System.Text;
using HWIDChecker.Services.Win32;
using HWIDChecker.Services.Models;
using static HWIDChecker.Services.Win32.SetupApi;

namespace HWIDChecker.Services
{
    public class DeviceCleaningService
    {
        public event Action<string> OnStatusUpdate;
        public event Action<string, string> OnError;

        private IntPtr _devicesHandle = IntPtr.Zero;

        public List<DeviceDetail> ScanForGhostDevices()
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

        public void RemoveGhostDevices(List<DeviceDetail> devices)
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
                if (_devicesHandle != IntPtr.Zero && _devicesHandle.ToInt64() != -1)
                {
                    SetupDiDestroyDeviceInfoList(_devicesHandle);
                    _devicesHandle = IntPtr.Zero;
                }
            }
        }

        [DllImport("setupapi.dll", SetLastError = true)]
        private static extern bool SetupDiDestroyDeviceInfoList(IntPtr DeviceInfoSet);
    }
}