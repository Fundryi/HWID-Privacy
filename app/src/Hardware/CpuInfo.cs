using System.Management;
using System.Runtime.Intrinsics.X86;
using System.Text;
using HWIDChecker.Services;

namespace HWIDChecker.Hardware;

public class CpuInfo : IHardwareInfo
{
    private readonly TextFormattingService textFormatter;

    public string SectionTitle => "CPU";

    public CpuInfo(TextFormattingService textFormatter = null)
    {
        this.textFormatter = textFormatter;
    }

    public string GetInformation()
    {
        var sb = new StringBuilder();
        using var searcher = new ManagementObjectSearcher("SELECT * FROM Win32_Processor");
        foreach (ManagementObject cpu in searcher.Get())
        {
            sb.AppendLine($"Name: {cpu["Name"]}");
            sb.AppendLine($"ProcessorId: {cpu["ProcessorId"]}");
            if (cpu["SerialNumber"] != null)
            {
                sb.AppendLine($"SerialNumber: {cpu["SerialNumber"]}");
            }
        }

        if (X86Base.IsSupported)
        {
            sb.AppendLine();

            var (eax0, ebx0, ecx0, edx0) = X86Base.CpuId(0, 0);
            int maxLeaf = eax0;

            var vendorBytes = new byte[12];
            BitConverter.GetBytes(ebx0).CopyTo(vendorBytes, 0);
            BitConverter.GetBytes(edx0).CopyTo(vendorBytes, 4);
            BitConverter.GetBytes(ecx0).CopyTo(vendorBytes, 8);
            string vendor = Encoding.ASCII.GetString(vendorBytes);
            sb.AppendLine($"CPUID Vendor: {vendor}");

            if (maxLeaf >= 1)
            {
                var (eax1, _, _, _) = X86Base.CpuId(1, 0);
                int stepping = eax1 & 0xF;
                int baseModel = (eax1 >> 4) & 0xF;
                int baseFamily = (eax1 >> 8) & 0xF;
                int extModel = (eax1 >> 16) & 0xF;
                int extFamily = (eax1 >> 20) & 0xFF;

                int family = baseFamily == 0xF ? baseFamily + extFamily : baseFamily;
                int model = (baseFamily == 0x6 || baseFamily == 0xF) ? (extModel << 4) | baseModel : baseModel;

                sb.AppendLine($"CPUID Signature (decoded): Family {family}, Model {model}, Stepping {stepping}");
            }

            if (maxLeaf >= 3)
            {
                var (_, _, ecx3, edx3) = X86Base.CpuId(3, 0);
                long serial = ((long)edx3 << 32) | (uint)ecx3;
                if (serial != 0)
                    sb.AppendLine($"CPUID Serial Number: {serial:X16}");
            }
        }

        return sb.ToString();
    }
}