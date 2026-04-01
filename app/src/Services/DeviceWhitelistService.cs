using System;
using System.Collections.Generic;
using System.IO;
using System.Text.Json;
using HWIDChecker.Services.Models;
using System.Text.Json.Serialization;

namespace HWIDChecker.Services
{
    public class DeviceWhitelistService
    {
        private readonly string _whitelistPath;

        public DeviceWhitelistService()
        {
            _whitelistPath = Path.Combine(Path.GetTempPath(), "hwid_device_whitelist.json");
        }

        public List<DeviceDetail> LoadWhitelistedDevices()
        {
            if (!File.Exists(_whitelistPath))
            {
                return new List<DeviceDetail>();
            }

            try
            {
                var json = File.ReadAllText(_whitelistPath);
                var options = new JsonSerializerOptions
                {
                    WriteIndented = true,
                    Converters = { new DeviceDetailConverter() }
                };
                return JsonSerializer.Deserialize<List<DeviceDetail>>(json, options);
            }
            catch
            {
                return new List<DeviceDetail>();
            }
        }

        public void SaveWhitelistedDevices(List<DeviceDetail> devices)
        {
            var options = new JsonSerializerOptions
            {
                WriteIndented = true,
                Converters = { new DeviceDetailConverter() }
            };
            var json = JsonSerializer.Serialize(devices, options);
            File.WriteAllText(_whitelistPath, json);
        }

        public bool IsDeviceWhitelisted(DeviceDetail device)
        {
            var whitelistedDevices = LoadWhitelistedDevices();
            return whitelistedDevices.Exists(d => d.HardwareId == device.HardwareId);
        }

        public void ResetWhitelist()
        {
            if (File.Exists(_whitelistPath))
            {
                File.Delete(_whitelistPath);
            }
        }
    }

    // Custom JSON converter to handle SP_DEVINFO_DATA serialization
    public class DeviceDetailConverter : JsonConverter<DeviceDetail>
    {
        public override DeviceDetail Read(ref Utf8JsonReader reader, Type typeToConvert, JsonSerializerOptions options)
        {
            using (JsonDocument document = JsonDocument.ParseValue(ref reader))
            {
                var root = document.RootElement;
                return new DeviceDetail(
                    root.GetProperty("Name").GetString(),
                    root.GetProperty("Description").GetString(),
                    root.GetProperty("HardwareId").GetString(),
                    root.GetProperty("Class").GetString(),
                    new Win32.SetupApi.SP_DEVINFO_DATA()); // Initialize with default since we don't need it for whitelist
            }
        }

        public override void Write(Utf8JsonWriter writer, DeviceDetail value, JsonSerializerOptions options)
        {
            writer.WriteStartObject();
            writer.WriteString("Name", value.Name);
            writer.WriteString("Description", value.Description);
            writer.WriteString("HardwareId", value.HardwareId);
            writer.WriteString("Class", value.Class);
            writer.WriteEndObject();
        }
    }
}