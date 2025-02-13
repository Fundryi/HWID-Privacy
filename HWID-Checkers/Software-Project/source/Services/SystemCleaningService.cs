using System;
using System.Collections.Generic;
using System.Threading.Tasks;
using HWIDChecker.Services.Models;

namespace HWIDChecker.Services
{
    public class SystemCleaningService
    {
        public event Action<string> OnStatusUpdate;
        public event Action<string, string> OnError;

        private readonly EventLogCleaningService _eventLogCleaner;
        private readonly DeviceCleaningService _deviceCleaner;

        public SystemCleaningService()
        {
            _eventLogCleaner = new EventLogCleaningService();
            _deviceCleaner = new DeviceCleaningService();

            // Forward events
            _eventLogCleaner.OnStatusUpdate += message => OnStatusUpdate?.Invoke(message);
            _eventLogCleaner.OnError += (source, message) => OnError?.Invoke(source, message);
            _deviceCleaner.OnStatusUpdate += message => OnStatusUpdate?.Invoke(message);
            _deviceCleaner.OnError += (source, message) => OnError?.Invoke(source, message);
        }

        public async Task CleanLogsAsync()
        {
            try
            {
                await _eventLogCleaner.CleanEventLogsAsync();
            }
            catch (Exception ex)
            {
                OnError?.Invoke("Cleaning Process", ex.Message);
                throw;
            }
        }

        public async Task<List<Models.DeviceDetail>> ScanForGhostDevicesAsync()
        {
            return await Task.Run(() => _deviceCleaner.ScanForGhostDevices());
        }

        public async Task RemoveGhostDevicesAsync(List<Models.DeviceDetail> devices)
        {
            await Task.Run(() => _deviceCleaner.RemoveGhostDevices(devices));
        }

        // For backward compatibility, use the same field layout as the shared model
        public struct DeviceDetail
        {
            private readonly Models.DeviceDetail _detail;

            public string Name => _detail.Name;
            public string Description => _detail.Description;
            public string HardwareId => _detail.HardwareId;
            public string Class => _detail.Class;
            public Win32.SetupApi.SP_DEVINFO_DATA DeviceInfoData => _detail.DeviceInfoData;

            public DeviceDetail(string name, string description, string hardwareId, string deviceClass, Win32.SetupApi.SP_DEVINFO_DATA deviceInfoData)
            {
                _detail = new Models.DeviceDetail(name, description, hardwareId, deviceClass, deviceInfoData);
            }

            public static implicit operator DeviceDetail(Models.DeviceDetail detail)
            {
                return new DeviceDetail(
                    detail.Name,
                    detail.Description,
                    detail.HardwareId,
                    detail.Class,
                    detail.DeviceInfoData);
            }

            public static implicit operator Models.DeviceDetail(DeviceDetail detail)
            {
                return detail._detail;
            }
        }
    }
}