using System.Collections.Generic;

namespace HWIDChecker.Services.Strategies
{
    public class DiskDriveIdentifierStrategy : BaseHardwareIdentifierStrategy
    {
        public override string[] GetComparisonProperties() => new[] { "Model", "Serial Number", "Size" };
    }

    public class RamIdentifierStrategy : BaseHardwareIdentifierStrategy
    {
        public override string[] GetComparisonProperties() => new[] { "Part Number", "Serial Number", "Size" };
    }

    public class CpuIdentifierStrategy : BaseHardwareIdentifierStrategy
    {
        public override string[] GetComparisonProperties() => new[] { "Name", "ProcessorId", "Manufacturer" };
    }

    public class MotherboardIdentifierStrategy : BaseHardwareIdentifierStrategy
    {
        public override string[] GetComparisonProperties() => new[] { "Product", "Serial Number", "Manufacturer" };
    }

    public class BiosIdentifierStrategy : BaseHardwareIdentifierStrategy
    {
        public override string[] GetComparisonProperties() => new[] { "Version", "Manufacturer", "Release Date" };
    }

    public class GpuIdentifierStrategy : BaseHardwareIdentifierStrategy
    {
        public override string[] GetComparisonProperties() => new[] { "Name", "DriverVersion", "VideoProcessor" };
    }

    public class TpmIdentifierStrategy : BaseHardwareIdentifierStrategy
    {
        public override string[] GetComparisonProperties() => new[] { "ManufacturerId", "ManufacturerVersion", "ManufacturerVersionInfo" };
    }

    public class UsbIdentifierStrategy : BaseHardwareIdentifierStrategy
    {
        public override string[] GetComparisonProperties() => new[] { "Description", "DeviceID", "Manufacturer" };
    }

    public class MonitorIdentifierStrategy : BaseHardwareIdentifierStrategy
    {
        public override string[] GetComparisonProperties() => new[] { "Name", "MonitorID", "MonitorModel" };
    }

    public class NetworkAdapterIdentifierStrategy : BaseHardwareIdentifierStrategy
    {
        public override string[] GetComparisonProperties() => new[] { "Name", "AdapterType", "MACAddress" };
    }
}