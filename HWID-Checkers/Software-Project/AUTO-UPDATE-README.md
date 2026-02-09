# Auto-Update System for HWID Checker

## Overview
The HWID Checker includes an auto-update system that compares the local executable hash with the GitHub-hosted executable hash, then updates in place when they differ.

## How It Works

### Update Detection
- The updater downloads `HWIDChecker.exe` from GitHub (raw URL, cache-busted query string)
- It computes SHA1 of the downloaded content
- It computes SHA1 of the currently running executable
- If hashes differ, it offers update/install

### Manual Update Check
- Users can click the "Check Updates" button to manually check for updates
- The system will show a confirmation dialog if an update is available
- If no update is available, it displays an informational message

### Update Process
1. **Detect**: Compare local SHA1 vs GitHub file SHA1
2. **Prompt**: Ask user to confirm update
3. **Download**: Stream new executable to temp path with progress UI
4. **Replace**: Generate temporary batch script to replace the running exe
5. **Restart**: Launch new executable and exit old process

## Technical Details

### Files Added/Modified
- `Services/AutoUpdateService.cs` - Core update logic
- `UI/Forms/SectionedViewForm.cs` - Main UI update button + event handler
- `HWIDChecker.csproj` - Project publish/runtime settings and package references

### GitHub Endpoints Used
- `https://github.com/Fundryi/HWID-Privacy/raw/main/HWIDChecker.exe`

### Update Logic
```csharp
// Download GitHub file and compute SHA1
var githubFileSha = await GetGitHubFileSha1Async();

// Compute local executable SHA1
var localFileSha = GetLocalFileSha();

// Update when hashes differ
if (!localFileSha.Equals(githubFileSha, StringComparison.OrdinalIgnoreCase))
{
    return await PerformUpdateAsync(githubFileSha);
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
- **Simple detection logic** - Direct binary hash comparison
- **No installer required** - In-place executable replacement
- **Seamless user flow** - One-click check/update from app UI
- **Cache-resistant fetch** - Cache-busting query parameter on download URL

## Error Handling
- Network connection issues are handled gracefully
- Update failures don't crash the application
- Users can continue using the current version if updates fail
- Clear error messages for troubleshooting
