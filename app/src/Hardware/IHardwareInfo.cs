using System.Text;

namespace HWIDChecker.Hardware;

public interface IHardwareInfo
{
    string GetInformation();
    string SectionTitle { get; }
}