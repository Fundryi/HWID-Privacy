using System;
using System.Collections.Generic;
using System.Runtime.InteropServices;
using System.Text;

namespace HWIDChecker.Services.Win32
{
    public static class SetupApi
    {
        private static readonly IntPtr INVALID_HANDLE_VALUE = new IntPtr(-1);

        [DllImport("setupapi.dll", CharSet = CharSet.Auto)]
        public static extern IntPtr SetupDiGetClassDevs(ref Guid ClassGuid, IntPtr Enumerator, IntPtr hwndParent, uint Flags);

        [DllImport("setupapi.dll", CharSet = CharSet.Auto, SetLastError = true)]
        public static extern bool SetupDiEnumDeviceInfo(IntPtr DeviceInfoSet, uint MemberIndex, ref SP_DEVINFO_DATA DeviceInfoData);

        [DllImport("setupapi.dll", CharSet = CharSet.Auto, SetLastError = true)]
        public static extern bool SetupDiGetDeviceRegistryProperty(
            IntPtr deviceInfoSet,
            ref SP_DEVINFO_DATA deviceInfoData,
            uint property,
            out uint propertyRegDataType,
            byte[] propertyBuffer,
            uint propertyBufferSize,
            out uint requiredSize);

        [DllImport("setupapi.dll", CharSet = CharSet.Auto, SetLastError = true)]
        public static extern bool SetupDiRemoveDevice(IntPtr DeviceInfoSet, ref SP_DEVINFO_DATA DeviceInfoData);

        [DllImport("setupapi.dll", SetLastError = true)]
        public static extern bool SetupDiDestroyDeviceInfoList(IntPtr DeviceInfoSet);

        [DllImport("setupapi.dll", CharSet = CharSet.Auto, SetLastError = true)]
        public static extern bool SetupDiGetDeviceInstanceId(
            IntPtr DeviceInfoSet,
            ref SP_DEVINFO_DATA DeviceInfoData,
            StringBuilder DeviceInstanceId,
            uint DeviceInstanceIdSize,
            out uint RequiredSize);

        [StructLayout(LayoutKind.Sequential)]
        public struct SP_DEVINFO_DATA
        {
            public uint cbSize;
            public Guid classGuid;
            public uint devInst;
            public IntPtr reserved;
        }

        [Flags]
        public enum DiGetClassFlags : uint
        {
            DIGCF_ALLCLASSES = 0x00000004,
            DIGCF_PRESENT = 0x00000002,
        }

        public enum SetupDiGetDeviceRegistryPropertyEnum : uint
        {
            SPDRP_DEVICEDESC = 0x00000000,
            SPDRP_HARDWAREID = 0x00000001,
            SPDRP_FRIENDLYNAME = 0x0000000C,
            SPDRP_CLASS = 0x00000007,
            SPDRP_INSTALL_STATE = 0x00000022,
        }

        /// <summary>
        /// Builds a cached map of PNPDeviceID → first Hardware ID string for all present devices.
        /// Call once, reuse across providers. Returns empty dictionary on failure.
        /// </summary>
        public static Dictionary<string, string> GetHardwareIdMap()
        {
            var map = new Dictionary<string, string>(StringComparer.OrdinalIgnoreCase);

            var emptyGuid = Guid.Empty;
            IntPtr deviceInfoSet = SetupDiGetClassDevs(
                ref emptyGuid,
                IntPtr.Zero,
                IntPtr.Zero,
                (uint)(DiGetClassFlags.DIGCF_ALLCLASSES | DiGetClassFlags.DIGCF_PRESENT));

            if (deviceInfoSet == INVALID_HANDLE_VALUE)
                return map;

            try
            {
                var deviceInfoData = new SP_DEVINFO_DATA();
                deviceInfoData.cbSize = (uint)Marshal.SizeOf<SP_DEVINFO_DATA>();

                for (uint i = 0; SetupDiEnumDeviceInfo(deviceInfoSet, i, ref deviceInfoData); i++)
                {
                    // Get device instance ID (the PNPDeviceID)
                    var instanceIdBuffer = new StringBuilder(512);
                    if (!SetupDiGetDeviceInstanceId(deviceInfoSet, ref deviceInfoData,
                            instanceIdBuffer, (uint)instanceIdBuffer.Capacity, out _))
                        continue;

                    string instanceId = instanceIdBuffer.ToString();

                    // Get SPDRP_HARDWAREID (REG_MULTI_SZ — null-separated strings)
                    byte[] buffer = new byte[2048];
                    if (!SetupDiGetDeviceRegistryProperty(deviceInfoSet, ref deviceInfoData,
                            (uint)SetupDiGetDeviceRegistryPropertyEnum.SPDRP_HARDWAREID,
                            out _, buffer, (uint)buffer.Length, out uint requiredSize))
                        continue;

                    // REG_MULTI_SZ: extract first string (most specific hardware ID)
                    string hardwareId = Encoding.Unicode.GetString(buffer, 0, (int)requiredSize).Split('\0')[0];
                    if (!string.IsNullOrEmpty(hardwareId))
                    {
                        map[instanceId] = hardwareId;
                    }
                }
            }
            finally
            {
                SetupDiDestroyDeviceInfoList(deviceInfoSet);
            }

            return map;
        }
    }
}
