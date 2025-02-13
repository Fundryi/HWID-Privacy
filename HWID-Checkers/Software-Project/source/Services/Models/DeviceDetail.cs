using System;
using HWIDChecker.Services.Win32;
using static HWIDChecker.Services.Win32.SetupApi;

namespace HWIDChecker.Services.Models
{
    public struct DeviceDetail
    {
        public string Name { get; }
        public string Description { get; }
        public string HardwareId { get; }
        public string Class { get; }
        public SP_DEVINFO_DATA DeviceInfoData;

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
