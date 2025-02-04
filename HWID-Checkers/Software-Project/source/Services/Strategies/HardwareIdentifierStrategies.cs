using System.Collections.Generic;

namespace HWIDChecker.Services.Strategies
{
    public class DiskDriveIdentifierStrategy : BaseHardwareIdentifierStrategy
    {
        public override string[] GetComparisonProperties() => new[] { "PhysicalDrive", "SerialNumber", "Size", "Model" };
        protected override string[] GetPrimaryKeyComponents() => new[] { "PhysicalDrive", "SerialNumber" };
        protected override string[] GetFallbackKeyComponents() => new[] { "DevicePath", "Size" };
    }

    public class RamIdentifierStrategy : BaseHardwareIdentifierStrategy
    {
        public override string[] GetComparisonProperties() => new[] { "DeviceLocator", "SerialNumber", "Capacity", "Speed" };
        protected override string[] GetPrimaryKeyComponents() => new[] { "DeviceLocator", "SerialNumber" };
        protected override string[] GetFallbackKeyComponents() => new[] { "Type", "Capacity", "Speed", "DeviceLocator" };
    }

    public class CpuIdentifierStrategy : BaseHardwareIdentifierStrategy
    {
        public override string[] GetComparisonProperties() => new[] { "ProcessorId", "Name", "Manufacturer" };
        protected override string[] GetPrimaryKeyComponents() => new[] { "ProcessorId" };
        protected override string[] GetFallbackKeyComponents() => new[] { "Name", "Manufacturer" };
    }

    public class GpuIdentifierStrategy : BaseHardwareIdentifierStrategy
    {
        public override string[] GetComparisonProperties() => new[] { "UUID", "DeviceId", "VendorId", "Name" };
        protected override string[] GetPrimaryKeyComponents() => new[] { "UUID" };
        protected override string[] GetFallbackKeyComponents() => new[] { "DeviceId", "VendorId" };
    }

    public class NetworkAdapterIdentifierStrategy : BaseHardwareIdentifierStrategy
    {
        public override string[] GetComparisonProperties() => new[] { "MACAddress", "AdapterType", "DriverVersion", "Name" };
        protected override string[] GetPrimaryKeyComponents() => new[] { "MACAddress" };
        protected override string[] GetFallbackKeyComponents() => new[] { "AdapterType", "DriverVersion" };
    }

    public class MotherboardIdentifierStrategy : BaseHardwareIdentifierStrategy
    {
        public override string[] GetComparisonProperties() => new[] { "SerialNumber", "Product", "Manufacturer" };
        protected override string[] GetPrimaryKeyComponents() => new[] { "SerialNumber" };
        protected override string[] GetFallbackKeyComponents() => new[] { "Product", "Manufacturer" };
    }

    public class BiosIdentifierStrategy : BaseHardwareIdentifierStrategy
    {
        public override string[] GetComparisonProperties() => new[] { "Version", "Manufacturer", "ReleaseDate" };
        protected override string[] GetPrimaryKeyComponents() => new[] { "Version", "Manufacturer" };
        protected override string[] GetFallbackKeyComponents() => new[] { "ReleaseDate" };
    }

    public class MonitorIdentifierStrategy : BaseHardwareIdentifierStrategy
    {
        public override string[] GetComparisonProperties() => new[] { "SerialNumber", "ProductCode", "Name" };
        protected override string[] GetPrimaryKeyComponents() => new[] { "SerialNumber" };
        protected override string[] GetFallbackKeyComponents() => new[] { "ProductCode", "Name" };
    }

    public class TpmIdentifierStrategy : BaseHardwareIdentifierStrategy
    {
        public override string[] GetComparisonProperties() => new[] { "ManufacturerId", "ManufacturerVersion", "ManufacturerVersionInfo" };
        protected override string[] GetPrimaryKeyComponents() => new[] { "ManufacturerId", "ManufacturerVersion" };
        protected override string[] GetFallbackKeyComponents() => new[] { "ManufacturerVersionInfo" };
    }

    public class UsbIdentifierStrategy : BaseHardwareIdentifierStrategy
    {
        public override string[] GetComparisonProperties() => new[] { "DeviceID", "Description", "Manufacturer" };
        protected override string[] GetPrimaryKeyComponents() => new[] { "DeviceID" };
        protected override string[] GetFallbackKeyComponents() => new[] { "Description", "Manufacturer" };
    }
}