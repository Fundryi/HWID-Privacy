using System;
using System.ComponentModel;
using System.IO;
using System.Runtime.InteropServices;
using System.Text;
using Microsoft.Win32.SafeHandles;

namespace HWIDChecker.Services.Win32;

/// <summary>
/// Helper class for querying WWN (World Wide Name) from physical drives using IOCTL_STORAGE_QUERY_PROPERTY.
/// </summary>
internal static class StorageDeviceIdQuery
{
    #region Windows API Constants

    private const uint GENERIC_READ = 0x80000000;
    private const uint FILE_SHARE_READ = 0x00000001;
    private const uint FILE_SHARE_WRITE = 0x00000002;
    private const uint FILE_SHARE_DELETE = 0x00000004;
    private const uint OPEN_EXISTING = 3;
    private const uint IOCTL_STORAGE_QUERY_PROPERTY = 0x002D1400;
    private const uint IOCTL_DISK_GET_DRIVE_LAYOUT_EX = 0x00070050;
    private const uint ERROR_INSUFFICIENT_BUFFER = 122;
    private const uint ERROR_MORE_DATA = 234;

    #endregion

    #region Windows API Structures

    [StructLayout(LayoutKind.Sequential)]
    private struct STORAGE_PROPERTY_QUERY
    {
        public STORAGE_PROPERTY_ID PropertyId;
        public STORAGE_QUERY_TYPE QueryType;
        // AdditionalParameters is a variable-length array, we don't need it for our query
    }

    private enum STORAGE_PROPERTY_ID : uint
    {
        StorageDeviceProperty = 0,
        StorageAdapterProperty = 1,
        StorageDeviceIdProperty = 2,
        StorageDeviceUniqueIdProperty = 3,
        StorageDeviceWriteCacheProperty = 4,
        StorageMiniportProperty = 5,
        StorageAccessAlignmentProperty = 6,
        StorageDeviceSeekPenaltyProperty = 7,
        StorageDeviceTrimProperty = 8,
        StorageDeviceWriteAggregationProperty = 9,
        StorageDeviceDeviceTelemetryProperty = 10,
        StorageDeviceLBProvisioningProperty = 11,
        StorageDevicePowerProperty = 12,
        StorageDeviceCopyOffloadProperty = 13,
        StorageDeviceResiliencyProperty = 14,
        StorageDeviceMediumProductType = 15,
        StorageAdapterRpmbProperty = 16,
        StorageAdapterCryptoProperty = 17,
        StorageDeviceIoCapabilityProperty = 18,
        StorageAdapterProtocolSpecificProperty = 19,
        StorageDeviceProtocolSpecificProperty = 20,
        StorageAdapterTemperatureProperty = 21,
        StorageDeviceTemperatureProperty = 22,
        StorageAdapterPhysicalTopologyProperty = 23,
        StorageDevicePhysicalTopologyProperty = 24,
        StorageDeviceAttributesProperty = 25,
        StorageDeviceManagementStatus = 26,
        StorageAdapterSerialNumberProperty = 27,
        StorageDeviceLocationProperty = 28,
        StorageDeviceNumaProperty = 29,
        StorageDeviceZonedDeviceProperty = 30,
        StorageDeviceUnsafeShutdownCount = 31,
        StorageDeviceEnduranceInfo = 32,
        StorageDeviceLedStateProperty = 33,
        StorageDeviceSelfEncryptionProperty = 34,
        StorageFruIdProperty = 35
    }

    private enum STORAGE_QUERY_TYPE : uint
    {
        PropertyStandardQuery = 0,
        PropertyExistsQuery = 1,
        PropertyMaskQuery = 2,
        PropertyQueryMaxDefined = 3
    }

    [StructLayout(LayoutKind.Sequential)]
    private struct STORAGE_DEVICE_ID_DESCRIPTOR
    {
        public uint Version;
        public uint Size;
        public uint NumberOfIdentifiers;
        // Identifiers follow this structure
    }

    [StructLayout(LayoutKind.Sequential)]
    private struct STORAGE_IDENTIFIER
    {
        public STORAGE_IDENTIFIER_CODE_SET CodeSet;
        public STORAGE_IDENTIFIER_TYPE Type;
        public ushort IdentifierSize;
        public ushort NextOffset;
        public ushort Association;
        public byte[] Identifier;

        public STORAGE_IDENTIFIER(byte[] data, int offset)
        {
            // Read fields from the data buffer
            CodeSet = (STORAGE_IDENTIFIER_CODE_SET)data[offset];
            Type = (STORAGE_IDENTIFIER_TYPE)data[offset + 1];
            IdentifierSize = BitConverter.ToUInt16(data, offset + 2);
            NextOffset = BitConverter.ToUInt16(data, offset + 4);
            Association = BitConverter.ToUInt16(data, offset + 6);
            
            // Read identifier bytes
            Identifier = new byte[IdentifierSize];
            if (IdentifierSize > 0 && offset + 8 + IdentifierSize <= data.Length)
            {
                Array.Copy(data, offset + 8, Identifier, 0, IdentifierSize);
            }
        }
    }

    private enum STORAGE_IDENTIFIER_CODE_SET : byte
    {
        CodeSetReserved = 0,
        CodeSetBinary = 1,
        CodeSetAscii = 2,
        CodeSetUtf8 = 3
    }

    private enum STORAGE_IDENTIFIER_TYPE : byte
    {
        VendorSpecific = 0,
        VendorId = 1,
        EUI64 = 2,
        FCPHName = 3,
        PortRelative = 4,
        TargetPortGroup = 5,
        LogicalUnitGroup = 6,
        MD5LogicalUnitIdentifier = 7,
        SCSINameString = 8
    }

    #endregion

    #region Windows API P/Invoke

    [DllImport("kernel32.dll", SetLastError = true, CharSet = CharSet.Auto)]
    private static extern SafeFileHandle CreateFile(
        string lpFileName,
        uint dwDesiredAccess,
        uint dwShareMode,
        IntPtr lpSecurityAttributes,
        uint dwCreationDisposition,
        uint dwFlagsAndAttributes,
        IntPtr hTemplateFile);

    [DllImport("kernel32.dll", SetLastError = true)]
    private static extern uint GetLastError();

    [DllImport("kernel32.dll", SetLastError = true)]
    private static extern bool DeviceIoControl(
        SafeFileHandle hDevice,
        uint dwIoControlCode,
        in STORAGE_PROPERTY_QUERY lpInBuffer,
        int nInBufferSize,
        byte[] lpOutBuffer,
        int nOutBufferSize,
        out uint lpBytesReturned,
        IntPtr lpOverlapped);

    [DllImport("kernel32.dll", SetLastError = true)]
    private static extern bool DeviceIoControl(
        SafeFileHandle hDevice,
        uint dwIoControlCode,
        IntPtr lpInBuffer,
        int nInBufferSize,
        byte[] lpOutBuffer,
        int nOutBufferSize,
        out uint lpBytesReturned,
        IntPtr lpOverlapped);

    #endregion

    #region Public API

    /// <summary>
    /// Attempts to get the WWN (World Wide Name) from a physical drive.
    /// </summary>
    /// <param name="diskIndex">The physical disk index (0-based).</param>
    /// <param name="wwnHex">Output: Colon-separated hex representation of the WWN.</param>
    /// <param name="decoded">Output: Decoded ASCII string if the identifier is printable.</param>
    /// <returns>True if WWN was successfully retrieved, false otherwise.</returns>
    public static bool TryGetWwnHexFromPhysicalDrive(int diskIndex, out string wwnHex, out string decoded)
    {
        wwnHex = string.Empty;
        decoded = string.Empty;

        try
        {
            string path = $@"\\.\PHYSICALDRIVE{diskIndex}";
            
            using SafeFileHandle handle = CreateFile(
                path,
                GENERIC_READ,
                FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
                IntPtr.Zero,
                OPEN_EXISTING,
                0,
                IntPtr.Zero);

            if (handle.IsInvalid)
            {
                uint error = GetLastError();
                decoded = $"CreateFile failed: Error {error}";
                return false;
            }

            var query = new STORAGE_PROPERTY_QUERY
            {
                PropertyId = STORAGE_PROPERTY_ID.StorageDeviceIdProperty,
                QueryType = STORAGE_QUERY_TYPE.PropertyStandardQuery
            };

            int querySize = Marshal.SizeOf<STORAGE_PROPERTY_QUERY>();
            int bufferSize = 4096; // Initial buffer size for device ID descriptor
            const int maxAttempts = 4;

            for (int attempt = 0; attempt < maxAttempts; attempt++)
            {
                byte[] buffer = new byte[bufferSize];

                bool success = DeviceIoControl(
                    handle,
                    IOCTL_STORAGE_QUERY_PROPERTY,
                    in query,
                    querySize,
                    buffer,
                    bufferSize,
                    out uint bytesReturned,
                    IntPtr.Zero);

                if (!success)
                {
                    uint error = GetLastError();
                    if ((error == ERROR_INSUFFICIENT_BUFFER || error == ERROR_MORE_DATA) && attempt < maxAttempts - 1)
                    {
                        bufferSize *= 2;
                        continue;
                    }

                    decoded = $"DeviceIoControl failed: Error {error}";
                    return false;
                }

                if (bytesReturned == 0)
                {
                    decoded = "DeviceIoControl: No data returned";
                    return false;
                }

                if (bytesReturned >= bufferSize && attempt < maxAttempts - 1)
                {
                    bufferSize *= 2;
                    continue;
                }

                int dataLength = (int)Math.Min(bytesReturned, (uint)buffer.Length);
                if (dataLength < Marshal.SizeOf<STORAGE_DEVICE_ID_DESCRIPTOR>())
                {
                    decoded = $"DeviceIoControl: Insufficient data ({bytesReturned} bytes)";
                    return false;
                }

                byte[] parseBuffer = buffer;
                if (dataLength != buffer.Length)
                {
                    parseBuffer = new byte[dataLength];
                    Array.Copy(buffer, parseBuffer, dataLength);
                }

                if (!TryParseDescriptor(parseBuffer, dataLength, out STORAGE_DEVICE_ID_DESCRIPTOR descriptor))
                {
                    decoded = "DeviceIoControl: Invalid descriptor data";
                    return false;
                }

                if (descriptor.NumberOfIdentifiers == 0)
                {
                    return false;
                }

                // Find the best identifier
                STORAGE_IDENTIFIER? bestIdentifier = FindBestIdentifier(parseBuffer, dataLength, descriptor);

                if (!bestIdentifier.HasValue)
                {
                    return false;
                }

                // Convert to colon-separated hex
                wwnHex = BytesToColonHex(bestIdentifier.Value.Identifier);

                // Try to decode if it's ASCII or looks printable
                if (bestIdentifier.Value.CodeSet == STORAGE_IDENTIFIER_CODE_SET.CodeSetAscii ||
                    IsPrintableAscii(bestIdentifier.Value.Identifier))
                {
                    string asciiDecoded = Encoding.ASCII.GetString(bestIdentifier.Value.Identifier).TrimEnd('\0');
                    decoded = asciiDecoded;
                }
                // Only show WWN if it's NOT a 16-byte GUID (GUIDs are not real WWN)
                else if (bestIdentifier.Value.IdentifierSize == 16)
                {
                    // 16-byte binary identifier - treat as no WWN (GUID is not a real WWN)
                    wwnHex = string.Empty;
                    decoded = string.Empty;
                }

                return true;
            }

            decoded = "DeviceIoControl: Buffer too small";
            return false;
        }
        catch
        {
            // Silently fail on any error
            return false;
        }
    }

    /// <summary>
    /// Result of a disk partition layout query.
    /// </summary>
    public class DiskLayoutInfo
    {
        public bool IsGpt { get; set; }
        /// <summary>GPT Disk GUID, or null for MBR.</summary>
        public Guid? DiskGuid { get; set; }
        /// <summary>MBR disk signature (4 bytes), or null for GPT.</summary>
        public uint? MbrSignature { get; set; }
        /// <summary>Partition GUIDs for GPT disks.</summary>
        public List<Guid> PartitionGuids { get; set; } = new();
    }

    /// <summary>
    /// Gets the partition layout (GPT Disk GUID / MBR Signature) for a physical drive.
    /// </summary>
    public static DiskLayoutInfo TryGetDiskLayout(int diskIndex)
    {
        try
        {
            string path = $@"\\.\PHYSICALDRIVE{diskIndex}";

            using SafeFileHandle handle = CreateFile(
                path,
                GENERIC_READ,
                FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
                IntPtr.Zero,
                OPEN_EXISTING,
                0,
                IntPtr.Zero);

            if (handle.IsInvalid) return null;

            int bufferSize = 4096;
            const int maxAttempts = 4;

            for (int attempt = 0; attempt < maxAttempts; attempt++)
            {
                byte[] buffer = new byte[bufferSize];

                bool success = DeviceIoControl(
                    handle,
                    IOCTL_DISK_GET_DRIVE_LAYOUT_EX,
                    IntPtr.Zero,
                    0,
                    buffer,
                    bufferSize,
                    out uint bytesReturned,
                    IntPtr.Zero);

                if (!success)
                {
                    uint error = GetLastError();
                    if ((error == ERROR_INSUFFICIENT_BUFFER || error == ERROR_MORE_DATA) && attempt < maxAttempts - 1)
                    {
                        bufferSize *= 2;
                        continue;
                    }
                    return null;
                }

                if (bytesReturned < 8) return null;

                return ParseDriveLayout(buffer, (int)bytesReturned);
            }
        }
        catch { }

        return null;
    }

    private static DiskLayoutInfo ParseDriveLayout(byte[] buffer, int length)
    {
        // DRIVE_LAYOUT_INFORMATION_EX layout:
        // Offset 0: DWORD PartitionStyle (0=MBR, 1=GPT)
        // Offset 4: DWORD PartitionCount
        // Offset 8: union (MBR: 4-byte signature at offset 8; GPT: GUID at offset 8..23)
        // Offset 48: PARTITION_INFORMATION_EX array

        if (length < 48) return null;

        uint partitionStyle = BitConverter.ToUInt32(buffer, 0);
        uint partitionCount = BitConverter.ToUInt32(buffer, 4);

        var info = new DiskLayoutInfo();

        if (partitionStyle == 1) // GPT
        {
            info.IsGpt = true;
            // GPT_DRIVE_LAYOUT_INFORMATION starts at offset 8
            // Offset 8: GUID DiskId (16 bytes)
            byte[] guidBytes = new byte[16];
            Array.Copy(buffer, 8, guidBytes, 0, 16);
            info.DiskGuid = new Guid(guidBytes);

            // Parse partition entries for their GUIDs
            // PARTITION_INFORMATION_EX is 144 bytes each, starts at offset 48
            const int partEntrySize = 144;
            const int partArrayOffset = 48;

            for (int i = 0; i < (int)partitionCount; i++)
            {
                int entryOffset = partArrayOffset + (i * partEntrySize);
                if (entryOffset + partEntrySize > length) break;

                // Each PARTITION_INFORMATION_EX:
                // Offset 0: DWORD PartitionStyle
                // Offset 32: union — for GPT: GUID PartitionType (16), GUID PartitionId (16)
                uint entryStyle = BitConverter.ToUInt32(buffer, entryOffset);
                if (entryStyle != 1) continue; // Not GPT partition

                // PartitionId is at offset 48 within the entry (after PartitionType GUID at offset 32)
                int partIdOffset = entryOffset + 48;
                if (partIdOffset + 16 > length) break;

                byte[] partGuidBytes = new byte[16];
                Array.Copy(buffer, partIdOffset, partGuidBytes, 0, 16);
                var partGuid = new Guid(partGuidBytes);

                // Skip empty GUIDs (unused partition slots)
                if (partGuid != Guid.Empty)
                {
                    info.PartitionGuids.Add(partGuid);
                }
            }
        }
        else if (partitionStyle == 0) // MBR
        {
            info.IsGpt = false;
            info.MbrSignature = BitConverter.ToUInt32(buffer, 8);
        }
        else
        {
            return null; // Unknown partition style
        }

        return info;
    }

    #endregion

    #region Private Helper Methods

    private static bool TryParseDescriptor(byte[] buffer, int bufferLength, out STORAGE_DEVICE_ID_DESCRIPTOR descriptor)
    {
        descriptor = default;
        if (bufferLength < Marshal.SizeOf<STORAGE_DEVICE_ID_DESCRIPTOR>())
        {
            return false;
        }

        descriptor = new STORAGE_DEVICE_ID_DESCRIPTOR
        {
            Version = BitConverter.ToUInt32(buffer, 0),
            Size = BitConverter.ToUInt32(buffer, 4),
            NumberOfIdentifiers = BitConverter.ToUInt32(buffer, 8)
        };
        if (descriptor.Size == 0 || descriptor.Size > bufferLength)
        {
            return false;
        }

        return true;
    }

    private static STORAGE_IDENTIFIER? FindBestIdentifier(byte[] buffer, int bufferLength, STORAGE_DEVICE_ID_DESCRIPTOR descriptor)
    {
        int offset = (int)Marshal.SizeOf<STORAGE_DEVICE_ID_DESCRIPTOR>();

        STORAGE_IDENTIFIER? asciiNameIdentifier = null;
        STORAGE_IDENTIFIER? binary8Or16Identifier = null;
        STORAGE_IDENTIFIER? firstIdentifier = null;

        for (uint i = 0; i < descriptor.NumberOfIdentifiers; i++)
        {
            if (offset + 8 > bufferLength) // Minimum size check
            {
                break;
            }

            STORAGE_IDENTIFIER identifier = new STORAGE_IDENTIFIER(buffer, offset);

            if (firstIdentifier == null)
            {
                firstIdentifier = identifier;
            }

            // Priority 1: ASCII name-string style identifiers
            if (asciiNameIdentifier == null &&
                identifier.CodeSet == STORAGE_IDENTIFIER_CODE_SET.CodeSetAscii &&
                (identifier.Type == STORAGE_IDENTIFIER_TYPE.SCSINameString ||
                 identifier.Type == STORAGE_IDENTIFIER_TYPE.VendorId ||
                 identifier.IdentifierSize > 0))
            {
                asciiNameIdentifier = identifier;
            }

            // Priority 2: Binary with 8 or 16 bytes (EUI-64 / NGUID-style)
            // Only accept if NOT all zeros (all-zeros means no WWN available)
            if (binary8Or16Identifier == null &&
                identifier.CodeSet == STORAGE_IDENTIFIER_CODE_SET.CodeSetBinary &&
                (identifier.IdentifierSize == 8 || identifier.IdentifierSize == 16) &&
                !IsAllZeros(identifier.Identifier))
            {
                binary8Or16Identifier = identifier;
            }

            // Move to next identifier
            if (identifier.NextOffset == 0)
            {
                break;
            }

            offset += identifier.NextOffset;
            if (offset >= bufferLength)
            {
                break;
            }
        }

        // Return based on priority
        return asciiNameIdentifier ?? binary8Or16Identifier ?? firstIdentifier;
    }

    private static bool IsAllZeros(byte[] bytes)
    {
        if (bytes == null || bytes.Length == 0)
        {
            return true;
        }
        
        foreach (byte b in bytes)
        {
            if (b != 0)
            {
                return false;
            }
        }
        return true;
    }

    private static string BytesToColonHex(byte[] bytes)
    {
        if (bytes == null || bytes.Length == 0)
        {
            return string.Empty;
        }

        var sb = new StringBuilder(bytes.Length * 3);
        for (int i = 0; i < bytes.Length; i++)
        {
            if (i > 0)
            {
                sb.Append(':');
            }
            sb.Append(bytes[i].ToString("X2"));
        }
        return sb.ToString();
    }

    private static bool IsPrintableAscii(byte[] bytes)
    {
        if (bytes == null || bytes.Length == 0)
        {
            return false;
        }

        foreach (byte b in bytes)
        {
            // Check if printable ASCII (space through tilde, or null terminator)
            if (b != 0 && (b < 32 || b > 126))
            {
                return false;
            }
        }
        return true;
    }

    #endregion
}
