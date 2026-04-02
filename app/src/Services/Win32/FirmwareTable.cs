using System;
using System.Collections.Generic;
using System.Runtime.InteropServices;
using System.Text;

namespace HWIDChecker.Services.Win32;

/// <summary>
/// Reads raw SMBIOS tables via GetSystemFirmwareTable and parses key structure types.
/// </summary>
internal static class FirmwareTable
{
    private const uint RSMB = 0x52534D42; // 'RSMB' provider signature

    [DllImport("kernel32.dll", SetLastError = true)]
    private static extern uint GetSystemFirmwareTable(
        uint FirmwareTableProviderSignature,
        uint FirmwareTableID,
        IntPtr pFirmwareTableBuffer,
        uint BufferSize);

    /// <summary>Parsed SMBIOS data for a specific structure type.</summary>
    public class SmbiosData
    {
        public string BiosVendor { get; set; }
        public string BiosVersion { get; set; }
        public string BiosReleaseDate { get; set; }

        public string SystemManufacturer { get; set; }
        public string SystemProduct { get; set; }
        public string SystemVersion { get; set; }
        public string SystemSerial { get; set; }
        public string SystemUuid { get; set; }
        public string SystemSku { get; set; }
        public string SystemFamily { get; set; }

        public string BoardManufacturer { get; set; }
        public string BoardProduct { get; set; }
        public string BoardVersion { get; set; }
        public string BoardSerial { get; set; }
        public string BoardAssetTag { get; set; }
        public string BoardLocation { get; set; }

        public string ChassisManufacturer { get; set; }
        public string ChassisType { get; set; }
        public string ChassisVersion { get; set; }
        public string ChassisSerial { get; set; }
        public string ChassisAssetTag { get; set; }
    }

    /// <summary>
    /// Reads and parses the raw SMBIOS table. Returns null on failure.
    /// </summary>
    public static SmbiosData GetSmbiosData()
    {
        try
        {
            byte[] rawTable = GetRawSmbiosTable();
            if (rawTable == null || rawTable.Length < 8)
                return null;

            // Raw SMBIOS data header:
            // Offset 0: BYTE Used20CallingMethod
            // Offset 1: BYTE SMBIOSMajorVersion
            // Offset 2: BYTE SMBIOSMinorVersion
            // Offset 3: BYTE DmiRevision
            // Offset 4-7: DWORD Length
            // Offset 8+: SMBIOS table data
            uint tableLength = BitConverter.ToUInt32(rawTable, 4);
            if (tableLength == 0 || 8 + tableLength > rawTable.Length)
                return null;

            return ParseSmbiosStructures(rawTable, 8, (int)(8 + tableLength));
        }
        catch
        {
            return null;
        }
    }

    private static byte[] GetRawSmbiosTable()
    {
        // First call: get required buffer size
        uint size = GetSystemFirmwareTable(RSMB, 0, IntPtr.Zero, 0);
        if (size == 0)
            return null;

        IntPtr buffer = Marshal.AllocHGlobal((int)size);
        try
        {
            uint written = GetSystemFirmwareTable(RSMB, 0, buffer, size);
            if (written == 0 || written != size)
                return null;

            byte[] result = new byte[size];
            Marshal.Copy(buffer, result, 0, (int)size);
            return result;
        }
        finally
        {
            Marshal.FreeHGlobal(buffer);
        }
    }

    private static SmbiosData ParseSmbiosStructures(byte[] data, int start, int end)
    {
        var result = new SmbiosData();
        int offset = start;

        while (offset < end - 4)  // Need at least 4 bytes for structure header
        {
            byte type = data[offset];
            byte length = data[offset + 1];

            // Sanity check: structure length must be at least 4
            if (length < 4 || offset + length > end)
                break;

            // Extract strings from the string table that follows the formatted area
            var strings = ExtractStrings(data, offset + length, end, out int nextOffset);

            switch (type)
            {
                case 0: // BIOS Information
                    ParseType0(data, offset, length, strings, result);
                    break;
                case 1: // System Information
                    ParseType1(data, offset, length, strings, result);
                    break;
                case 2: // Baseboard Information
                    ParseType2(data, offset, length, strings, result);
                    break;
                case 3: // Chassis Information
                    ParseType3(data, offset, length, strings, result);
                    break;
                case 127: // End-of-table
                    return result;
            }

            offset = nextOffset;
            if (offset <= 0)
                break; // Safety: prevent infinite loop
        }

        return result;
    }

    /// <summary>
    /// Extracts the null-terminated string table after a structure's formatted area.
    /// The string table is terminated by a double-null (empty string).
    /// </summary>
    private static List<string> ExtractStrings(byte[] data, int stringStart, int end, out int nextStructureOffset)
    {
        var strings = new List<string>();
        int pos = stringStart;

        while (pos < end)
        {
            if (data[pos] == 0)
            {
                // Double-null: end of string table
                nextStructureOffset = pos + 1;
                return strings;
            }

            // Find the end of this null-terminated string
            int strEnd = pos;
            while (strEnd < end && data[strEnd] != 0)
                strEnd++;

            if (strEnd > pos)
            {
                strings.Add(Encoding.ASCII.GetString(data, pos, strEnd - pos));
            }

            pos = strEnd + 1; // Skip the null terminator
        }

        // Reached end of buffer without double-null
        nextStructureOffset = end;
        return strings;
    }

    private static string GetString(List<string> strings, byte index)
    {
        // SMBIOS string indices are 1-based. Index 0 means "not set".
        if (index == 0 || index > strings.Count)
            return null;
        return strings[index - 1];
    }

    private static void ParseType0(byte[] data, int offset, int length, List<string> strings, SmbiosData result)
    {
        // Type 0 (BIOS Information):
        // Offset 04h: Vendor (string index)
        // Offset 05h: BIOS Version (string index)
        // Offset 08h: BIOS Release Date (string index)
        if (length > 4) result.BiosVendor = GetString(strings, data[offset + 4]);
        if (length > 5) result.BiosVersion = GetString(strings, data[offset + 5]);
        if (length > 8) result.BiosReleaseDate = GetString(strings, data[offset + 8]);
    }

    private static void ParseType1(byte[] data, int offset, int length, List<string> strings, SmbiosData result)
    {
        // Type 1 (System Information):
        // Offset 04h: Manufacturer (string)
        // Offset 05h: Product Name (string)
        // Offset 06h: Version (string)
        // Offset 07h: Serial Number (string)
        // Offset 08h-17h: UUID (16 bytes)
        // Offset 19h: SKU Number (string) — only in SMBIOS 2.4+
        // Offset 1Ah: Family (string) — only in SMBIOS 2.4+
        if (length > 4) result.SystemManufacturer = GetString(strings, data[offset + 4]);
        if (length > 5) result.SystemProduct = GetString(strings, data[offset + 5]);
        if (length > 6) result.SystemVersion = GetString(strings, data[offset + 6]);
        if (length > 7) result.SystemSerial = GetString(strings, data[offset + 7]);

        if (length >= 0x18) // UUID at offset 8, 16 bytes
        {
            // SMBIOS UUID format: first 3 fields are little-endian, rest are big-endian
            byte[] uuid = new byte[16];
            Array.Copy(data, offset + 8, uuid, 0, 16);
            result.SystemUuid = FormatSmbiosUuid(uuid);
        }

        if (length > 0x19) result.SystemSku = GetString(strings, data[offset + 0x19]);
        if (length > 0x1A) result.SystemFamily = GetString(strings, data[offset + 0x1A]);
    }

    private static void ParseType2(byte[] data, int offset, int length, List<string> strings, SmbiosData result)
    {
        // Type 2 (Baseboard Information):
        // Offset 04h: Manufacturer (string)
        // Offset 05h: Product (string)
        // Offset 06h: Version (string)
        // Offset 07h: Serial Number (string)
        // Offset 08h: Asset Tag (string)
        // Offset 0Ah: Location in Chassis (string)
        if (length > 4) result.BoardManufacturer = GetString(strings, data[offset + 4]);
        if (length > 5) result.BoardProduct = GetString(strings, data[offset + 5]);
        if (length > 6) result.BoardVersion = GetString(strings, data[offset + 6]);
        if (length > 7) result.BoardSerial = GetString(strings, data[offset + 7]);
        if (length > 8) result.BoardAssetTag = GetString(strings, data[offset + 8]);
        if (length > 0x0A) result.BoardLocation = GetString(strings, data[offset + 0x0A]);
    }

    private static void ParseType3(byte[] data, int offset, int length, List<string> strings, SmbiosData result)
    {
        // Type 3 (Chassis Information):
        // Offset 04h: Manufacturer (string)
        // Offset 05h: Type (byte — enum)
        // Offset 06h: Version (string)
        // Offset 07h: Serial Number (string)
        // Offset 08h: Asset Tag (string)
        if (length > 4) result.ChassisManufacturer = GetString(strings, data[offset + 4]);
        if (length > 5) result.ChassisType = DecodeChassisType(data[offset + 5]);
        if (length > 6) result.ChassisVersion = GetString(strings, data[offset + 6]);
        if (length > 7) result.ChassisSerial = GetString(strings, data[offset + 7]);
        if (length > 8) result.ChassisAssetTag = GetString(strings, data[offset + 8]);
    }

    private static string FormatSmbiosUuid(byte[] uuid)
    {
        // SMBIOS 2.6+ UUID: first 3 groups are little-endian, last 2 are big-endian
        // Format: XXXXXXXX-XXXX-XXXX-XXXX-XXXXXXXXXXXX
        return $"{uuid[3]:X2}{uuid[2]:X2}{uuid[1]:X2}{uuid[0]:X2}-" +
               $"{uuid[5]:X2}{uuid[4]:X2}-" +
               $"{uuid[7]:X2}{uuid[6]:X2}-" +
               $"{uuid[8]:X2}{uuid[9]:X2}-" +
               $"{uuid[10]:X2}{uuid[11]:X2}{uuid[12]:X2}{uuid[13]:X2}{uuid[14]:X2}{uuid[15]:X2}";
    }

    private static string DecodeChassisType(byte type)
    {
        // Bit 7 is the lock flag, mask it out
        byte chassisType = (byte)(type & 0x7F);
        return chassisType switch
        {
            1 => "Other",
            2 => "Unknown",
            3 => "Desktop",
            4 => "Low Profile Desktop",
            5 => "Pizza Box",
            6 => "Mini Tower",
            7 => "Tower",
            8 => "Portable",
            9 => "Laptop",
            10 => "Notebook",
            11 => "Hand Held",
            12 => "Docking Station",
            13 => "All in One",
            14 => "Sub Notebook",
            15 => "Space-saving",
            16 => "Lunch Box",
            17 => "Main Server Chassis",
            23 => "Rack Mount Chassis",
            24 => "Sealed-case PC",
            30 => "Tablet",
            31 => "Convertible",
            32 => "Detachable",
            33 => "IoT Gateway",
            34 => "Embedded PC",
            35 => "Mini PC",
            36 => "Stick PC",
            _ => $"Type {chassisType}"
        };
    }
}
