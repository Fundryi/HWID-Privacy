# Auto-Update System for HWID Checker

## Overview
The HWID Checker now includes an auto-update system that can check for and download the latest version from GitHub without requiring manual releases.

## How It Works

### Update Detection
- The system checks the GitHub API for commits that modified the `HWIDChecker.exe` file
- It compares the commit timestamp with the current executable's last modified time
- If a newer version exists on GitHub, it offers to download and install it

### Manual Update Check
- Users can click the "Check Updates" button to manually check for updates
- The system will show a confirmation dialog if an update is available
- If no update is available, it displays an informational message

### Update Process
1. **Detection**: Checks GitHub API for latest commit that modified `HWIDChecker.exe`
2. **Download**: Downloads the new executable to a temporary location
3. **Replace**: Creates a batch script to replace the current executable
4. **Restart**: Automatically restarts the application with the new version

## Technical Details

### Files Added/Modified
- `Services/AutoUpdateService.cs` - Core update logic
- `UI/Forms/MainFormLayout.cs` - Added "Check Updates" button
- `UI/Forms/MainFormEventHandlers.cs` - Update button event handler
- `UI/Forms/MainFormInitializer.cs` - Integration with main form
- `HWIDChecker.csproj` - Added System.Text.Json dependency

### GitHub API Endpoints Used
- `https://api.github.com/repos/Fundryi/HWID-Privacy/commits?path=HWIDChecker.exe&per_page=1`
- `https://github.com/Fundryi/HWID-Privacy/raw/main/HWIDChecker.exe`

### Update Logic
```csharp
// Gets the latest commit that modified HWIDChecker.exe
var latestCommitInfo = await GetLatestCommitForFileAsync();

// Compares with current file timestamp
var currentFileTime = GetCurrentExecutableTime();

// Updates if GitHub version is newer
if (latestCommitInfo.CommitDate > currentFileTime)
{
    return await PerformUpdateAsync(latestCommitInfo.Sha, latestCommitInfo.CommitDate);
}
```

## Deployment Workflow

### For Developers
1. Make changes to the code
2. Build the project: `dotnet publish -c Release`
3. Copy the new `HWIDChecker.exe` to the repository root
4. Commit and push to GitHub
5. The update system will automatically detect the new version

### For Users
1. Open HWID Checker
2. Click "Check Updates" button
3. If an update is available, click "Yes" to download and install
4. The application will restart automatically with the new version

## Benefits
- **No manual releases required** - Updates are based on commits
- **Automatic file replacement** - No manual download/extract needed
- **Seamless user experience** - One-click update process
- **Reliable detection** - Uses GitHub's commit API for accuracy

## Error Handling
- Network connection issues are handled gracefully
- Update failures don't crash the application
- Users can continue using the current version if updates fail
- Clear error messages for troubleshooting