# HWID Checker Project

> This whole project is 99.99% written by AI.

## Table of Contents

- [HWID Checker Project](#hwid-checker-project)
  - [Table of Contents](#table-of-contents)
  - [Building the Project](#building-the-project)
  - [Requirements](#requirements)
  - [Features](#features)
    - [Core Functionality](#core-functionality)
    - [Hardware Detection](#hardware-detection)
    - [System Services](#system-services)
  - [Usage Instructions](#usage-instructions)
    - [GUI Version](#gui-version)
    - [Command Line](#command-line)
  - [Project Structure](#project-structure)
  - [File Formats](#file-formats)
    - [Export Files (TXT)](#export-files-txt)
    - [Comparison Results](#comparison-results)

## Building the Project

```bash
dotnet publish "HWID-Checkers/Software-Project/source/HWIDChecker.csproj" -c Release
```

This will:

- Build the project in Release configuration
- Create a single-file executable
- Copy published files to `source/bin/RELEASE/win-x64/publish`

## Requirements

- .NET 8.0 SDK
- Windows 10/11 (x64)
- Administrator privileges for hardware detection

## Features

### Core Functionality

- Hardware ID validation for Windows 10/11
- Cross-version compatibility checks
- Comprehensive hardware component detection
- Real-time validation and comparison

### Hardware Detection

- Detection of 20+ hardware components including:
  - CPU information
  - GPU details
  - Storage devices
  - Network adapters
  - BIOS information
  - TPM status
  - USB devices
  - System information
  - Monitor details
  - RAM configuration
  - Motherboard information
- Real-time refresh capability
- WMI-based data collection

### System Services

- Hardware information management
- Component comparison service
- File export functionality
- System cleaning utilities
- Device management
- Event log maintenance

## Usage Instructions

### GUI Version

1. Run `HWIDChecker.exe` as Administrator
2. Click "Scan Hardware" to detect components
3. Use Export/Compare features as needed
4. Optional: Use cleaning tools for system maintenance

### Command Line

Located in HWID-Checkers/Bats/:
```bat
:: Windows 10 check
HWID CHECK W10.bat

:: Windows 11 check
HWID CHECK W11.bat
```

## Project Structure

```
HWID-Checkers/Software-Project/
├── source/
│   ├── Hardware/              # Hardware component implementations
│   │   ├── ArpInfo.cs
│   │   ├── BiosInfo.cs
│   │   ├── CpuInfo.cs
│   │   ├── DiskDriveInfo.cs
│   │   ├── GpuInfo.cs
│   │   ├── MonitorInfo.cs
│   │   ├── MotherboardInfo.cs
│   │   ├── NetworkInfo.cs
│   │   ├── RamInfo.cs
│   │   ├── SystemInfo.cs
│   │   ├── TpmInfo.cs
│   │   └── UsbInfo.cs
│   ├── Services/             # Core services
│   │   ├── Interfaces/       # Service contracts
│   │   ├── Models/          # Data models
│   │   ├── Strategies/      # Implementation strategies
│   │   └── Win32/          # Native Windows API integration
│   ├── UI/                  # User interface components
│   │   ├── Components/      # Reusable UI elements
│   │   ├── DataHandlers/    # UI data management
│   │   └── Forms/          # Windows Forms
│   └── Utils/              # Helper utilities
├── ComparisonSystem-Architecture.md  # System architecture documentation
└── README.md                         # Project documentation

../Bats/                    # Batch script utilities
    ├── HWID CHECK W10.bat  # Windows 10 validation
    └── HWID CHECK W11.bat  # Windows 11 validation
```

## File Formats

### Export Files (TXT)

- Header with system metadata
- Sections for each hardware component:
  - Component name
  - Manufacturer
  - Hardware IDs
  - Component-specific details
  - Detection timestamp

### Comparison Results

- JSON-based diff format
- Machine-readable change log
- Visual highlighting of modifications
- Detailed component comparisons
