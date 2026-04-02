using System;
using System.Runtime.InteropServices;
using System.Text;

namespace HWIDChecker.Services.Win32;

/// <summary>
/// P/Invoke wrapper for NVIDIA NVAPI. NVAPI exports a single function (nvapi_QueryInterface)
/// that returns function pointers by numeric interface ID. We resolve each needed function
/// at runtime via Marshal.GetDelegateForFunctionPointer.
/// </summary>
internal static class NvApi
{
    #region Interface IDs

    private const uint ID_NvAPI_Initialize = 0x0150E828;
    private const uint ID_NvAPI_EnumPhysicalGPUs = 0xE5AC921F;
    private const uint ID_NvAPI_GPU_GetBoardInfo = 0x22D54523;
    private const uint ID_NvAPI_Unload = 0xD22BDD7E;

    #endregion

    #region Native Import

    [DllImport("nvapi64.dll", EntryPoint = "nvapi_QueryInterface", SetLastError = true)]
    private static extern IntPtr NvAPI_QueryInterface(uint id);

    #endregion

    #region Delegate Types

    [UnmanagedFunctionPointer(CallingConvention.Cdecl)]
    private delegate int NvAPI_InitializeDelegate();

    [UnmanagedFunctionPointer(CallingConvention.Cdecl)]
    private delegate int NvAPI_EnumPhysicalGPUsDelegate(
        [Out] IntPtr[] gpuHandles,
        out int gpuCount);

    [UnmanagedFunctionPointer(CallingConvention.Cdecl)]
    private delegate int NvAPI_GPU_GetBoardInfoDelegate(
        IntPtr gpuHandle,
        ref NV_BOARD_INFO boardInfo);

    [UnmanagedFunctionPointer(CallingConvention.Cdecl)]
    private delegate int NvAPI_UnloadDelegate();

    #endregion

    #region Structures

    [StructLayout(LayoutKind.Sequential)]
    private struct NV_BOARD_INFO
    {
        public uint version;

        [MarshalAs(UnmanagedType.ByValArray, SizeConst = 16)]
        public byte[] BoardNum;
    }

    private const int NV_BOARD_INFO_SIZE = 4 + 16; // version (4) + BoardNum (16)
    private const uint NV_BOARD_INFO_VER1 = NV_BOARD_INFO_SIZE | (1 << 16);

    #endregion

    #region Public API

    /// <summary>
    /// Attempts to retrieve the board part number from the first NVIDIA GPU via NVAPI.
    /// Returns false if NVAPI is unavailable, no GPU is found, or the call fails.
    /// </summary>
    public static bool TryGetBoardInfo(out string boardPartNumber)
    {
        boardPartNumber = string.Empty;

        try
        {
            // Resolve function pointers
            IntPtr pInitialize = NvAPI_QueryInterface(ID_NvAPI_Initialize);
            if (pInitialize == IntPtr.Zero) return false;

            IntPtr pEnumGPUs = NvAPI_QueryInterface(ID_NvAPI_EnumPhysicalGPUs);
            if (pEnumGPUs == IntPtr.Zero) return false;

            IntPtr pGetBoardInfo = NvAPI_QueryInterface(ID_NvAPI_GPU_GetBoardInfo);
            if (pGetBoardInfo == IntPtr.Zero) return false;

            IntPtr pUnload = NvAPI_QueryInterface(ID_NvAPI_Unload);

            var initialize = Marshal.GetDelegateForFunctionPointer<NvAPI_InitializeDelegate>(pInitialize);
            var enumGPUs = Marshal.GetDelegateForFunctionPointer<NvAPI_EnumPhysicalGPUsDelegate>(pEnumGPUs);
            var getBoardInfo = Marshal.GetDelegateForFunctionPointer<NvAPI_GPU_GetBoardInfoDelegate>(pGetBoardInfo);
            NvAPI_UnloadDelegate unload = pUnload != IntPtr.Zero
                ? Marshal.GetDelegateForFunctionPointer<NvAPI_UnloadDelegate>(pUnload)
                : null;

            try
            {
                int status = initialize();
                if (status != 0) return false;

                IntPtr[] gpuHandles = new IntPtr[64]; // NVAPI_MAX_PHYSICAL_GPUS
                status = enumGPUs(gpuHandles, out int gpuCount);
                if (status != 0 || gpuCount == 0) return false;

                var boardInfo = new NV_BOARD_INFO
                {
                    version = NV_BOARD_INFO_VER1,
                    BoardNum = new byte[16]
                };

                status = getBoardInfo(gpuHandles[0], ref boardInfo);
                if (status != 0) return false;

                if (boardInfo.BoardNum == null || boardInfo.BoardNum.Length == 0)
                    return false;

                // Board part number is ASCII, null-terminated
                string partNum = Encoding.ASCII.GetString(boardInfo.BoardNum).TrimEnd('\0');
                if (string.IsNullOrWhiteSpace(partNum) || partNum.Trim('0') == string.Empty)
                    return false;

                boardPartNumber = partNum;
                return true;
            }
            finally
            {
                unload?.Invoke();
            }
        }
        catch (DllNotFoundException)
        {
            return false;
        }
        catch (EntryPointNotFoundException)
        {
            return false;
        }
        catch
        {
            return false;
        }
    }

    #endregion
}
