using System;
using System.Collections.Generic;
using System.Linq;
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
#pragma warning disable CS0067 // The event is never used
        public event Action<string, string> OnError;
#pragma warning restore CS0067

        private const int ERROR_INSUFFICIENT_BUFFER = 122;

        private static readonly HashSet<string> IgnoredHardwareIds = new(StringComparer.OrdinalIgnoreCase)
        {
            @"SW\{96E080C7-143C-11D1-B40F-00A0C9223196}", // Microsoft Streaming Service Proxy
            "ms_pppoeminiport",      // WAN Miniport (PPPOE)
            "ms_pptpminiport",       // WAN Miniport (PPTP)
            "ms_agilevpnminiport",   // WAN Miniport (IKEv2)
            "ms_ndiswanbh",          // WAN Miniport (Network Monitor)
            "ms_ndiswanip",          // WAN Miniport (IP)
            "ms_sstpminiport",       // WAN Miniport (SSTP)
            "ms_ndiswanipv6",        // WAN Miniport (IPv6)
            "ms_l2tpminiport",       // WAN Miniport (L2TP)
            @"MMDEVAPI\AudioEndpoints" // Audio Endpoint
        };

        private IntPtr _devicesHandle = IntPtr.Zero;

        private static bool IsInvalidDeviceHandle(IntPtr handle) => handle == IntPtr.Zero || handle.ToInt64() == -1;

        public List<DeviceDetail> ScanForGhostDevices()
        {
            var devices = new List<DeviceDetail>();
            var setupClass = Guid.Empty;
            
            _devicesHandle = SetupDiGetClassDevs(ref setupClass, IntPtr.Zero, IntPtr.Zero, (uint)DiGetClassFlags.DIGCF_ALLCLASSES);

            if (IsInvalidDeviceHandle(_devicesHandle))
            {
                throw new Exception("Failed to get device list");
            }

            try
            {
                uint deviceIndex = 0;
                var deviceInfoData = new SP_DEVINFO_DATA { cbSize = (uint)Marshal.SizeOf(typeof(SP_DEVINFO_DATA)) };

                while (SetupDiEnumDeviceInfo(_devicesHandle, deviceIndex, ref deviceInfoData))
                {
                    var properties = ReadDeviceProperties(ref deviceInfoData);
                    bool isGhostDevice = IsGhostDevice(ref deviceInfoData, properties);

                    if (isGhostDevice)
                    {
                        var deviceInfoCopy = new SP_DEVINFO_DATA
                        {
                            cbSize = deviceInfoData.cbSize,
                            classGuid = deviceInfoData.classGuid,
                            devInst = deviceInfoData.devInst,
                            reserved = deviceInfoData.reserved
                        };

                        var deviceName = properties.GetValueOrDefault(SetupDiGetDeviceRegistryPropertyEnum.SPDRP_FRIENDLYNAME) ??
                                         properties.GetValueOrDefault(SetupDiGetDeviceRegistryPropertyEnum.SPDRP_DEVICEDESC) ??
                                         "Unknown Device";
                        var deviceDesc = properties.GetValueOrDefault(SetupDiGetDeviceRegistryPropertyEnum.SPDRP_DEVICEDESC) ??
                                         deviceName;

                        var hardwareIds = GetHardwareIds(properties);
                        if (ShouldIgnoreDevice(hardwareIds))
                        {
                            deviceIndex++;
                            continue;
                        }

                        var deviceClass = properties.GetValueOrDefault(SetupDiGetDeviceRegistryPropertyEnum.SPDRP_CLASS) ?? "";
                        var hardwareId = hardwareIds.Count > 0 ? string.Join(" | ", hardwareIds) : "";

                        devices.Add(new DeviceDetail(
                            deviceName,
                            deviceDesc,
                            hardwareId,
                            deviceClass,
                            deviceInfoCopy));
                    }

                    deviceIndex++;
                }

                return devices;
            }
            catch
            {
                if (!IsInvalidDeviceHandle(_devicesHandle))
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
                if (IsInvalidDeviceHandle(_devicesHandle))
                {
                    throw new Exception("Invalid device list handle. Please scan for devices first.");
                }

                OnStatusUpdate?.Invoke($"\r\nAttempting to remove {devices.Count} ghost device(s)...\r\n");
                int removedCount = 0;

                foreach (var device in devices)
                {
                    try
                    {
                        var devInfoData = device.DeviceInfoData;
                        
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
                if (!IsInvalidDeviceHandle(_devicesHandle))
                {
                    SetupDiDestroyDeviceInfoList(_devicesHandle);
                    _devicesHandle = IntPtr.Zero;
                }
            }
        }

        private Dictionary<SetupDiGetDeviceRegistryPropertyEnum, string> ReadDeviceProperties(ref SP_DEVINFO_DATA deviceInfoData)
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
                if (!TryReadDeviceProperty(ref deviceInfoData, prop, out _, out var propBuffer, out var requiredSize))
                {
                    continue;
                }

                if (prop == SetupDiGetDeviceRegistryPropertyEnum.SPDRP_INSTALL_STATE)
                {
                    if (requiredSize >= sizeof(uint))
                    {
                        properties[prop] = BitConverter.ToUInt32(propBuffer, 0).ToString();
                    }
                }
                else if (requiredSize > 0)
                {
                    properties[prop] = Encoding.Unicode.GetString(propBuffer, 0, (int)requiredSize).Trim('\0');
                }
            }

            return properties;
        }

        private bool TryReadDeviceProperty(
            ref SP_DEVINFO_DATA deviceInfoData,
            SetupDiGetDeviceRegistryPropertyEnum prop,
            out uint propType,
            out byte[] propBuffer,
            out uint requiredSize)
        {
            propType = 0;
            requiredSize = 0;
            propBuffer = new byte[1024];

            if (SetupDiGetDeviceRegistryProperty(
                    _devicesHandle,
                    ref deviceInfoData,
                    (uint)prop,
                    out propType,
                    propBuffer,
                    (uint)propBuffer.Length,
                    out requiredSize))
            {
                return true;
            }

            int error = Marshal.GetLastWin32Error();
            if (error != ERROR_INSUFFICIENT_BUFFER || requiredSize == 0)
            {
                return false;
            }

            propBuffer = new byte[requiredSize];
            return SetupDiGetDeviceRegistryProperty(
                _devicesHandle,
                ref deviceInfoData,
                (uint)prop,
                out propType,
                propBuffer,
                requiredSize,
                out requiredSize);
        }

        private static List<string> GetHardwareIds(Dictionary<SetupDiGetDeviceRegistryPropertyEnum, string> properties)
        {
            if (!properties.TryGetValue(SetupDiGetDeviceRegistryPropertyEnum.SPDRP_HARDWAREID, out var hwIdsRaw) ||
                string.IsNullOrWhiteSpace(hwIdsRaw))
            {
                return new List<string>();
            }

            return hwIdsRaw
                .Split('\0', StringSplitOptions.RemoveEmptyEntries)
                .Select(id => id.Trim())
                .Where(id => !string.IsNullOrWhiteSpace(id))
                .ToList();
        }

        private static bool ShouldIgnoreDevice(List<string> hardwareIds)
        {
            if (hardwareIds.Count == 0)
            {
                return false;
            }

            return hardwareIds.Any(id => IgnoredHardwareIds.Contains(id));
        }

        private static bool IsGhostDevice(ref SP_DEVINFO_DATA deviceInfoData, Dictionary<SetupDiGetDeviceRegistryPropertyEnum, string> properties)
        {
            if (CM_Get_DevNode_Status(out uint devNodeStatus, out _, deviceInfoData.devInst, 0) == CR_SUCCESS)
            {
                return (devNodeStatus & DN_PRESENT) == 0;
            }

            if (properties.TryGetValue(SetupDiGetDeviceRegistryPropertyEnum.SPDRP_INSTALL_STATE, out var installStateRaw) &&
                uint.TryParse(installStateRaw, out uint installState))
            {
                return installState != 0;
            }

            return false;
        }

        [DllImport("setupapi.dll", SetLastError = true)]
        private static extern bool SetupDiDestroyDeviceInfoList(IntPtr DeviceInfoSet);
    }
}
