using System;
using System.Net;
using System.Net.Sockets;
using System.Runtime.InteropServices;

namespace HWIDChecker.Services.Win32;

internal static class IpHlpApi
{
    [DllImport("iphlpapi.dll", SetLastError = true)]
    private static extern int GetIpNetTable2(AddressFamily Family, out IntPtr Table);

    [DllImport("iphlpapi.dll")]
    private static extern void FreeMibTable(IntPtr Table);

    // NL_NEIGHBOR_STATE enum
    private const int NlnsReachable = 0;
    private const int NlnsPermanent = 5;
    private const int NlnsUnreachable = 1;

    // AF_UNSPEC to get both IPv4 and IPv6
    private const int AF_UNSPEC = 0;

    public class NeighborEntry
    {
        public string IpAddress { get; set; }
        public string PhysicalAddress { get; set; }
        public uint InterfaceIndex { get; set; }
        public bool IsIpv6 { get; set; }
    }

    /// <summary>
    /// Gets the neighbor cache (ARP table for IPv4, NDP for IPv6) using GetIpNetTable2.
    /// Returns null on failure (caller should use arp.exe fallback).
    /// </summary>
    public static List<NeighborEntry> GetNeighborTable()
    {
        IntPtr tablePtr = IntPtr.Zero;
        try
        {
            int result = GetIpNetTable2((AddressFamily)AF_UNSPEC, out tablePtr);
            if (result != 0 || tablePtr == IntPtr.Zero)
                return null;

            // MIB_IPNET_TABLE2 layout:
            // Offset 0: ULONG NumEntries
            // Offset 8: MIB_IPNET_ROW2[] (aligned)
            uint numEntries = (uint)Marshal.ReadInt32(tablePtr, 0);
            if (numEntries == 0)
                return new List<NeighborEntry>();

            // MIB_IPNET_ROW2 starts at offset 8 (after NumEntries + padding)
            int rowSize = GetMibIpNetRow2Size();
            IntPtr rowPtr = IntPtr.Add(tablePtr, 8);

            var entries = new List<NeighborEntry>();
            for (uint i = 0; i < numEntries; i++)
            {
                var entry = ParseRow(rowPtr);
                if (entry != null)
                    entries.Add(entry);

                rowPtr = IntPtr.Add(rowPtr, rowSize);
            }

            return entries;
        }
        catch
        {
            return null;
        }
        finally
        {
            if (tablePtr != IntPtr.Zero)
                FreeMibTable(tablePtr);
        }
    }

    private static NeighborEntry ParseRow(IntPtr rowPtr)
    {
        // MIB_IPNET_ROW2 layout (x64):
        // Offset  0: SOCKADDR_INET Address (28 bytes)
        // Offset 28: NET_IFINDEX InterfaceIndex (4 bytes)
        // Offset 32: NET_LUID InterfaceLuid (8 bytes)
        // Offset 40: UCHAR PhysicalAddress[32] (32 bytes)
        // Offset 72: ULONG PhysicalAddressLength (4 bytes)
        // Offset 76: NL_NEIGHBOR_STATE State (4 bytes)
        // ... flags follow

        // Read address family from SOCKADDR_INET (first 2 bytes)
        short af = Marshal.ReadInt16(rowPtr, 0);
        bool isIpv6 = (af == (short)AddressFamily.InterNetworkV6);
        bool isIpv4 = (af == (short)AddressFamily.InterNetwork);

        if (!isIpv4 && !isIpv6)
            return null;

        // Read IP address
        string ipAddress;
        if (isIpv4)
        {
            // SOCKADDR_IN: offset 4 = sin_addr (4 bytes)
            byte[] addrBytes = new byte[4];
            Marshal.Copy(IntPtr.Add(rowPtr, 4), addrBytes, 0, 4);
            ipAddress = new IPAddress(addrBytes).ToString();
        }
        else
        {
            // SOCKADDR_IN6: offset 8 = sin6_addr (16 bytes)
            byte[] addrBytes = new byte[16];
            Marshal.Copy(IntPtr.Add(rowPtr, 8), addrBytes, 0, 16);
            ipAddress = new IPAddress(addrBytes).ToString();
        }

        uint interfaceIndex = (uint)Marshal.ReadInt32(rowPtr, 28);

        // Physical address (MAC)
        uint macLength = (uint)Marshal.ReadInt32(rowPtr, 72);
        int state = Marshal.ReadInt32(rowPtr, 76);

        // Skip unreachable and permanent (static) entries
        if (state == NlnsUnreachable || macLength == 0)
            return null;

        byte[] macBytes = new byte[macLength];
        Marshal.Copy(IntPtr.Add(rowPtr, 40), macBytes, 0, (int)macLength);
        string mac = string.Join(":", macBytes.Select(b => b.ToString("X2")));

        // Filter broadcast and multicast
        if (mac == "FF:FF:FF:FF:FF:FF") return null;
        if (macBytes.Length > 0 && (macBytes[0] & 0x01) != 0) return null; // Multicast bit set

        return new NeighborEntry
        {
            IpAddress = ipAddress,
            PhysicalAddress = mac,
            InterfaceIndex = interfaceIndex,
            IsIpv6 = isIpv6
        };
    }

    /// <summary>
    /// Returns the size of MIB_IPNET_ROW2. This varies by platform but is 88 on x64 Windows 10/11.
    /// </summary>
    private static int GetMibIpNetRow2Size()
    {
        // MIB_IPNET_ROW2 on x64:
        // SOCKADDR_INET (28) + InterfaceIndex (4) + InterfaceLuid (8) +
        // PhysicalAddress[32] (32) + PhysicalAddressLength (4) + State (4) +
        // union flags (4) + ReachabilityTime (4) = 88 bytes
        return 88;
    }
}
