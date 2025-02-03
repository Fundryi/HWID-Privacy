# HWID Checker Project

## Table of Contents

- [Building](#building-the-project)
- [Requirements](#requirements)
- [Features](#features)
- [Usage](#usage-instructions)
- [Project Structure](#project-structure)
- [File Formats](#file-formats)

## Building the Project

```bash
dotnet publish "HWID-Checkers/Software-Project/source/HWIDChecker.csproj" -c Releas
```

This will:

- Build the project in Release configuration
- Create a single-file executable
- Copy published files to `HWID-Checkers/Software-Project/source/bin/RELEASE/win-x64/publish`

## Requirements

- .NET 8.0 SDK
- Windows 10/11 (x64)
- Administrator privileges for hardware detection

## Features

### Core Functionality

- Hardware ID validation for Windows 10/11
- Cross-version compatibility checks
- BAT script interfaces for quick checks:
  - `HWID CHECK W10.bat`: Windows 10 specific validation
  - `HWID CHECK W11.bat`: Windows 11 specific validation

### Hardware Detection

- 20+ hardware components monitored
- Real-time refresh capability
- WMI-based data collection

### Export System

- Generates timestamped TXT files
- Automatic file management (retains last 10 exports)
- Standardized format for machine comparison

## Usage Instructions

### GUI Version

1. Run `HWIDChecker.exe`
2. Click "Scan Hardware"
3. Use Export/Compare buttons as needed

### Command Line

```bat
:: Windows 10 check
HWID CHECK W10.bat

:: Windows 11 check
HWID CHECK W11.bat
```

## Project Structure

```
HWID-CHECKER-Project/
├── HWID CHECK BATS/        # Command-line validation scripts
│   ├── HWID CHECK W10.bat
│   └── HWID CHECK W11.bat
├── source/
│   ├── Hardware/          # Hardware info classes (CPU, GPU, etc)
│   ├── Services/          # Comparison and export services
│   ├── UI/                # Windows Forms components
│   ├── Utils/             # Helper classes and extensions
│   └── HWIDChecker.csproj # Project configuration
└── HWIDChecker.exe        # Compiled executable
```

## File Formats

### Export Files (TXT)

- Header with system metadata
- Section for each hardware component:
  - Component name
  - Manufacturer
  - Hardware IDs
  - Detection timestamp

### Comparison Results

- JSON-based diff format
- Machine-readable change log
- Visual highlighting of modifications

```bat
:: Sample BAT file output
[HWID Check] Windows 11 Validation Report
[STATUS] PASSED - All hardware components meet requirements
[DETAILS] 18/18 validation checks successful
```

.
