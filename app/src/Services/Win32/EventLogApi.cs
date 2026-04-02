using System;
using System.Runtime.InteropServices;

namespace HWIDChecker.Services.Win32
{
    public static class EventLogApi
    {
        private const string WevtapiDll = "wevtapi.dll";
        private const string Advapi32Dll = "advapi32.dll";
        private const string Kernel32Dll = "kernel32.dll";

        // --- Handle management ---

        [DllImport(WevtapiDll, CharSet = CharSet.Unicode, SetLastError = true)]
        public static extern bool EvtClose(IntPtr Object);

        // --- Channel enumeration (replaces "wevtutil el") ---

        [DllImport(WevtapiDll, CharSet = CharSet.Unicode, SetLastError = true)]
        public static extern IntPtr EvtOpenChannelEnum(IntPtr Session, uint Flags);

        [DllImport(WevtapiDll, CharSet = CharSet.Unicode, SetLastError = true)]
        public static extern bool EvtNextChannelPath(
            IntPtr ChannelEnum,
            int ChannelPathBufferSize,
            [Out] char[] ChannelPathBuffer,
            out int ChannelPathBufferUsed);

        // --- Channel config (for enabled state) ---

        [DllImport(WevtapiDll, CharSet = CharSet.Unicode, SetLastError = true)]
        public static extern IntPtr EvtOpenChannelConfig(IntPtr Session, string ChannelPath, uint Flags);

        [DllImport(WevtapiDll, CharSet = CharSet.Unicode, SetLastError = true)]
        public static extern bool EvtGetChannelConfigProperty(
            IntPtr ChannelConfig,
            EvtChannelConfigPropertyId PropertyId,
            uint Flags,
            int PropertyValueBufferSize,
            IntPtr PropertyValueBuffer,
            out int PropertyValueBufferUsed);

        // --- Log info (for record count) ---

        [DllImport(WevtapiDll, CharSet = CharSet.Unicode, SetLastError = true)]
        public static extern IntPtr EvtOpenLog(IntPtr Session, string Path, EvtOpenLogFlags Flags);

        [DllImport(WevtapiDll, CharSet = CharSet.Unicode, SetLastError = true)]
        public static extern bool EvtGetLogInfo(
            IntPtr Log,
            EvtLogPropertyId PropertyId,
            int PropertyValueBufferSize,
            IntPtr PropertyValueBuffer,
            out int PropertyValueBufferUsed);

        // --- Channel config set (for disable/enable cycle) ---

        [DllImport(WevtapiDll, CharSet = CharSet.Unicode, SetLastError = true)]
        public static extern bool EvtSetChannelConfigProperty(
            IntPtr ChannelConfig,
            EvtChannelConfigPropertyId PropertyId,
            uint Flags,
            ref EVT_VARIANT PropertyValue);

        [DllImport(WevtapiDll, CharSet = CharSet.Unicode, SetLastError = true)]
        public static extern bool EvtSaveChannelConfig(IntPtr ChannelConfig, uint Flags);

        // --- Token privilege adjustment (for SeSecurityPrivilege) ---

        [DllImport(Advapi32Dll, SetLastError = true)]
        private static extern bool OpenProcessToken(IntPtr ProcessHandle, uint DesiredAccess, out IntPtr TokenHandle);

        [DllImport(Advapi32Dll, CharSet = CharSet.Unicode, SetLastError = true)]
        private static extern bool LookupPrivilegeValue(string lpSystemName, string lpName, out long lpLuid);

        [DllImport(Advapi32Dll, SetLastError = true)]
        private static extern bool AdjustTokenPrivileges(
            IntPtr TokenHandle, bool DisableAllPrivileges,
            ref TOKEN_PRIVILEGES NewState, int BufferLength,
            IntPtr PreviousState, IntPtr ReturnLength);

        [DllImport(Kernel32Dll)]
        private static extern IntPtr GetCurrentProcess();

        [DllImport(Kernel32Dll, SetLastError = true)]
        private static extern bool CloseHandle(IntPtr hObject);

        private const uint TOKEN_ADJUST_PRIVILEGES = 0x0020;
        private const uint TOKEN_QUERY = 0x0008;
        private const uint SE_PRIVILEGE_ENABLED = 0x00000002;

        [StructLayout(LayoutKind.Sequential)]
        private struct TOKEN_PRIVILEGES
        {
            public uint PrivilegeCount;
            public long Luid;
            public uint Attributes;
        }

        [StructLayout(LayoutKind.Sequential)]
        public struct EVT_VARIANT
        {
            public int BooleanVal;
            public int Padding;
            public int Count;
            public int Type;
        }

        // --- Clear (replaces "wevtutil cl") ---

        [DllImport(WevtapiDll, CharSet = CharSet.Unicode, SetLastError = true)]
        public static extern bool EvtClearLog(
            IntPtr Session,
            string ChannelPath,
            string TargetFilePath,
            uint Flags);

        // --- Enums ---

        public enum EvtOpenLogFlags
        {
            EvtOpenChannelPath = 0x1,
            EvtOpenFilePath = 0x2
        }

        public enum EvtLogPropertyId
        {
            EvtLogCreationTime = 0,
            EvtLogLastAccessTime = 1,
            EvtLogLastWriteTime = 2,
            EvtLogFileSize = 3,
            EvtLogAttributes = 4,
            EvtLogNumberOfLogRecords = 5,
            EvtLogNumberOfOldestRecord = 6,
            EvtLogFull = 7
        }

        public enum EvtChannelConfigPropertyId
        {
            EvtChannelConfigEnabled = 0,
            EvtChannelConfigIsolation = 1,
            EvtChannelConfigType = 2,
            EvtChannelConfigOwningPublisher = 3,
            EvtChannelConfigClassicEventlog = 4
        }

        // EVT_VARIANT type IDs (partial - only what we need)
        public const int EvtVarTypeNull = 0;
        public const int EvtVarTypeBoolean = 13;
        public const int EvtVarTypeUInt64 = 10;

        // EVT_VARIANT is 16 bytes: 8-byte value + 4-byte count + 4-byte type
        public const int EvtVariantSize = 16;

        // Channel types (EvtChannelConfigType values)
        public const int EvtChannelTypeAdmin = 0;
        public const int EvtChannelTypeOperational = 1;
        public const int EvtChannelTypeAnalytic = 2;
        public const int EvtChannelTypeDebug = 3;

        public const int ERROR_NO_MORE_ITEMS = 259;
        public const int ERROR_INSUFFICIENT_BUFFER = 122;
        public const int ERROR_ACCESS_DENIED = 5;

        // --- Helper methods ---

        /// <summary>
        /// Enumerate all channel names on the system. Returns the list directly, no process spawning.
        /// </summary>
        public static List<string> EnumerateChannels()
        {
            var channels = new List<string>();
            var enumHandle = EvtOpenChannelEnum(IntPtr.Zero, 0);
            if (enumHandle == IntPtr.Zero)
                return channels;

            try
            {
                var buffer = new char[512];
                while (true)
                {
                    if (EvtNextChannelPath(enumHandle, buffer.Length, buffer, out int used))
                    {
                        // used includes the null terminator
                        channels.Add(new string(buffer, 0, used - 1));
                    }
                    else
                    {
                        int err = Marshal.GetLastWin32Error();
                        if (err == ERROR_NO_MORE_ITEMS)
                            break;

                        if (err == ERROR_INSUFFICIENT_BUFFER)
                        {
                            buffer = new char[used];
                            if (EvtNextChannelPath(enumHandle, buffer.Length, buffer, out used))
                            {
                                channels.Add(new string(buffer, 0, used - 1));
                            }
                            else
                            {
                                break;
                            }
                        }
                        else
                        {
                            break;
                        }
                    }
                }
            }
            finally
            {
                EvtClose(enumHandle);
            }

            return channels;
        }

        /// <summary>
        /// Check if a channel is enabled. Returns null if the channel config cannot be opened.
        /// </summary>
        public static bool? IsChannelEnabled(string channelPath)
        {
            var configHandle = EvtOpenChannelConfig(IntPtr.Zero, channelPath, 0);
            if (configHandle == IntPtr.Zero)
                return null;

            try
            {
                var buffer = Marshal.AllocHGlobal(EvtVariantSize);
                try
                {
                    if (EvtGetChannelConfigProperty(
                            configHandle,
                            EvtChannelConfigPropertyId.EvtChannelConfigEnabled,
                            0,
                            EvtVariantSize,
                            buffer,
                            out _))
                    {
                        // EVT_VARIANT: first 4 bytes are the bool value for EvtVarTypeBoolean
                        int val = Marshal.ReadInt32(buffer);
                        return val != 0;
                    }
                    return null;
                }
                finally
                {
                    Marshal.FreeHGlobal(buffer);
                }
            }
            finally
            {
                EvtClose(configHandle);
            }
        }

        /// <summary>
        /// Get the number of records in a log channel. Returns null if the log cannot be queried
        /// (e.g. Debug/Analytic channels that don't support this property).
        /// </summary>
        public static ulong? GetLogRecordCount(string channelPath)
        {
            var logHandle = EvtOpenLog(IntPtr.Zero, channelPath, EvtOpenLogFlags.EvtOpenChannelPath);
            if (logHandle == IntPtr.Zero)
                return null;

            try
            {
                var buffer = Marshal.AllocHGlobal(EvtVariantSize);
                try
                {
                    if (EvtGetLogInfo(
                            logHandle,
                            EvtLogPropertyId.EvtLogNumberOfLogRecords,
                            EvtVariantSize,
                            buffer,
                            out _))
                    {
                        // EVT_VARIANT: first 8 bytes are the UInt64 value
                        long val = Marshal.ReadInt64(buffer);
                        return (ulong)val;
                    }
                    return null;
                }
                finally
                {
                    Marshal.FreeHGlobal(buffer);
                }
            }
            finally
            {
                EvtClose(logHandle);
            }
        }

        /// <summary>
        /// Clear a log channel. Returns true on success, false on failure.
        /// On failure, sets lastError to the Win32 error code.
        /// </summary>
        public static bool ClearLog(string channelPath, out int lastError)
        {
            bool result = EvtClearLog(IntPtr.Zero, channelPath, null, 0);
            lastError = result ? 0 : Marshal.GetLastWin32Error();
            return result;
        }

        /// <summary>
        /// Get the channel type (Admin=0, Operational=1, Analytic=2, Debug=3).
        /// Returns null if the config cannot be read.
        /// </summary>
        public static int? GetChannelType(string channelPath)
        {
            var configHandle = EvtOpenChannelConfig(IntPtr.Zero, channelPath, 0);
            if (configHandle == IntPtr.Zero)
                return null;

            try
            {
                var buffer = Marshal.AllocHGlobal(EvtVariantSize);
                try
                {
                    if (EvtGetChannelConfigProperty(
                            configHandle,
                            EvtChannelConfigPropertyId.EvtChannelConfigType,
                            0,
                            EvtVariantSize,
                            buffer,
                            out _))
                    {
                        return Marshal.ReadInt32(buffer);
                    }
                    return null;
                }
                finally
                {
                    Marshal.FreeHGlobal(buffer);
                }
            }
            finally
            {
                EvtClose(configHandle);
            }
        }

        /// <summary>
        /// Disable or enable a channel. Used for Debug/Analytic channels that must be
        /// disabled before clearing. Returns true on success.
        /// </summary>
        public static bool SetChannelEnabled(string channelPath, bool enabled)
        {
            var configHandle = EvtOpenChannelConfig(IntPtr.Zero, channelPath, 0);
            if (configHandle == IntPtr.Zero)
                return false;

            try
            {
                var variant = new EVT_VARIANT
                {
                    BooleanVal = enabled ? 1 : 0,
                    Type = EvtVarTypeBoolean
                };

                if (!EvtSetChannelConfigProperty(
                        configHandle,
                        EvtChannelConfigPropertyId.EvtChannelConfigEnabled,
                        0,
                        ref variant))
                    return false;

                return EvtSaveChannelConfig(configHandle, 0);
            }
            finally
            {
                EvtClose(configHandle);
            }
        }

        /// <summary>
        /// Enable a named privilege (e.g. "SeSecurityPrivilege") on the current process token.
        /// Returns true on success.
        /// </summary>
        public static bool EnablePrivilege(string privilegeName)
        {
            if (!OpenProcessToken(GetCurrentProcess(), TOKEN_ADJUST_PRIVILEGES | TOKEN_QUERY, out var tokenHandle))
                return false;

            try
            {
                if (!LookupPrivilegeValue(null, privilegeName, out long luid))
                    return false;

                var tp = new TOKEN_PRIVILEGES
                {
                    PrivilegeCount = 1,
                    Luid = luid,
                    Attributes = SE_PRIVILEGE_ENABLED
                };

                return AdjustTokenPrivileges(tokenHandle, false, ref tp, 0, IntPtr.Zero, IntPtr.Zero);
            }
            finally
            {
                CloseHandle(tokenHandle);
            }
        }
    }
}
