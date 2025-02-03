using System;
using System.Collections.Generic;
using System.Windows.Forms;
using HWIDChecker.Hardware;
using HWIDChecker.UI.Components;

namespace HWIDChecker.UI.DataHandlers
{
    public class HardwareDataHandlerFactory
    {
        private readonly Dictionary<string, HardwareDataHandler> handlers;

        public HardwareDataHandlerFactory(HardwareInfoManager hardwareInfoManager)
        {
            handlers = new Dictionary<string, HardwareDataHandler>
            {
                // Single-item handlers
                { "SYSTEM INFORMATION", new SystemInfoHandler(hardwareInfoManager) },
                { "BIOS INFORMATION", new BiosInfoHandler(hardwareInfoManager) },
                { "MOTHERBOARD INFORMATION", new MotherboardInfoHandler(hardwareInfoManager) },
                { "CPU INFORMATION", new CpuInfoHandler(hardwareInfoManager) },
                { "RAM INFORMATION", new RamInfoHandler(hardwareInfoManager) },
                { "GPU INFORMATION", new GpuInfoHandler(hardwareInfoManager) },
                { "TPM INFORMATION", new TpmInfoHandler(hardwareInfoManager) },
                { "ARP INFORMATION", new ArpInfoHandler(hardwareInfoManager) },

                // Multi-item handlers
                { "NETWORK INFORMATION", new NetworkInfoHandler(hardwareInfoManager) },
                { "DISK DRIVE INFORMATION", new DiskDriveInfoHandler(hardwareInfoManager) },
                { "MONITOR INFORMATION", new MonitorInfoHandler(hardwareInfoManager) },
                { "USB INFORMATION", new UsbInfoHandler(hardwareInfoManager) }
            };
        }

        public HardwareDataHandler GetHandler(string infoType)
        {
            if (handlers.TryGetValue(infoType, out var handler))
            {
                return handler;
            }

            throw new ArgumentException($"No handler found for info type: {infoType}");
        }
    }

    // Simple handlers for basic info types
    public class BiosInfoHandler : HardwareDataHandler
    {
        public BiosInfoHandler(HardwareInfoManager manager) : base(manager) { }
        public override void UpdateDisplay(RichTextBox textBox)
        {
            var info = hardwareInfoManager.GetSpecificHardwareInfo("BIOS INFORMATION");
            var data = ParseBasicInfo(info);
            UpdateSingleItemDisplay(textBox, data);
        }
    }

    public class MotherboardInfoHandler : HardwareDataHandler
    {
        public MotherboardInfoHandler(HardwareInfoManager manager) : base(manager) { }
        public override void UpdateDisplay(RichTextBox textBox)
        {
            var info = hardwareInfoManager.GetSpecificHardwareInfo("MOTHERBOARD INFORMATION");
            var data = ParseBasicInfo(info);
            UpdateSingleItemDisplay(textBox, data);
        }
    }

    public class CpuInfoHandler : HardwareDataHandler
    {
        public CpuInfoHandler(HardwareInfoManager manager) : base(manager) { }
        public override void UpdateDisplay(RichTextBox textBox)
        {
            var info = hardwareInfoManager.GetSpecificHardwareInfo("CPU INFORMATION");
            var data = ParseBasicInfo(info);
            UpdateSingleItemDisplay(textBox, data);
        }
    }

    public class RamInfoHandler : HardwareDataHandler
    {
        public RamInfoHandler(HardwareInfoManager manager) : base(manager) { }
        public override void UpdateDisplay(RichTextBox textBox)
        {
            var info = hardwareInfoManager.GetSpecificHardwareInfo("RAM INFORMATION");
            var data = ParseBasicInfo(info);
            UpdateSingleItemDisplay(textBox, data);
        }
    }

    public class GpuInfoHandler : HardwareDataHandler
    {
        public GpuInfoHandler(HardwareInfoManager manager) : base(manager) { }
        public override void UpdateDisplay(RichTextBox textBox)
        {
            var info = hardwareInfoManager.GetSpecificHardwareInfo("GPU INFORMATION");
            var data = ParseBasicInfo(info);
            UpdateSingleItemDisplay(textBox, data);
        }
    }

    public class TpmInfoHandler : HardwareDataHandler
    {
        public TpmInfoHandler(HardwareInfoManager manager) : base(manager) { }
        public override void UpdateDisplay(RichTextBox textBox)
        {
            var info = hardwareInfoManager.GetSpecificHardwareInfo("TPM INFORMATION");
            var data = ParseBasicInfo(info);
            UpdateSingleItemDisplay(textBox, data);
        }
    }

    public class ArpInfoHandler : HardwareDataHandler
    {
        public ArpInfoHandler(HardwareInfoManager manager) : base(manager) { }
        public override void UpdateDisplay(RichTextBox textBox)
        {
            var info = hardwareInfoManager.GetSpecificHardwareInfo("ARP INFORMATION");
            var data = ParseBasicInfo(info);
            UpdateSingleItemDisplay(textBox, data);
        }
    }

    public class DiskDriveInfoHandler : HardwareDataHandler
    {
        public DiskDriveInfoHandler(HardwareInfoManager manager) : base(manager) { }
        public override void UpdateDisplay(RichTextBox textBox)
        {
            var info = hardwareInfoManager.GetSpecificHardwareInfo("DISK DRIVE INFORMATION");
            var items = ParseMultiItemInfo(info);
            UpdateMultiItemDisplay(textBox, items);
        }
    }

    public class MonitorInfoHandler : HardwareDataHandler
    {
        public MonitorInfoHandler(HardwareInfoManager manager) : base(manager) { }
        public override void UpdateDisplay(RichTextBox textBox)
        {
            var info = hardwareInfoManager.GetSpecificHardwareInfo("MONITOR INFORMATION");
            var items = ParseMultiItemInfo(info);
            UpdateMultiItemDisplay(textBox, items);
        }
    }

    public class UsbInfoHandler : HardwareDataHandler
    {
        public UsbInfoHandler(HardwareInfoManager manager) : base(manager) { }
        public override void UpdateDisplay(RichTextBox textBox)
        {
            var info = hardwareInfoManager.GetSpecificHardwareInfo("USB INFORMATION");
            var items = ParseMultiItemInfo(info);
            UpdateMultiItemDisplay(textBox, items);
        }
    }
}