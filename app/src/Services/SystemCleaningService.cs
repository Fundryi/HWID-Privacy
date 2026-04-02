using System;
using System.Collections.Generic;
using System.Threading;
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

        public async Task CleanLogsAsync(CancellationToken cancellationToken = default)
        {
            try
            {
                await _eventLogCleaner.CleanEventLogsAsync(cancellationToken);
            }
            catch (OperationCanceledException)
            {
                // User-requested cancellation should not be treated as an error.
                throw;
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
    }
}
