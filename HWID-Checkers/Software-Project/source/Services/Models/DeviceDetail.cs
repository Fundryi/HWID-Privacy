using System;
using static HWIDChecker.Services.Win32.SetupApi;

namespace HWIDChecker.Services.Models
{
    public class DeviceDetail
    {
        public string Name { get; set; }
        public string Description { get; set; }
        public string HardwareId { get; set; }
        public string Class { get; set; }
        public SP_DEVINFO_DATA DeviceInfoData { get; set; }

        public DeviceDetail()
        {
        }

        public DeviceDetail(string name, string description, string hardwareId, string deviceClass, SP_DEVINFO_DATA deviceInfoData)
        {
            Name = name;
            Description = description;
            HardwareId = hardwareId;
            Class = deviceClass;
            DeviceInfoData = deviceInfoData;
        }
    }
}
