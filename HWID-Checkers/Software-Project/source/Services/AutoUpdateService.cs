using System;
using System.Diagnostics;
using System.IO;
using System.Net.Http;
using System.Text.Json;
using System.Threading.Tasks;
using System.Windows.Forms;

namespace HWIDChecker.Services
{
    public class AutoUpdateService
    {
        private const string GITHUB_API_COMMITS_URL = "https://api.github.com/repos/Fundryi/HWID-Privacy/commits";
        private const string GITHUB_RAW_URL = "https://github.com/Fundryi/HWID-Privacy/raw/main/HWIDChecker.exe";
        
        private readonly HttpClient httpClient;
        private readonly string currentDirectory;
        private readonly string currentExecutablePath;

        public AutoUpdateService()
        {
            httpClient = new HttpClient();
            httpClient.DefaultRequestHeaders.Add("User-Agent", "HWID-Checker-AutoUpdater");
            
            currentDirectory = Application.StartupPath;
            currentExecutablePath = Process.GetCurrentProcess().MainModule?.FileName ??
                                   Path.Combine(currentDirectory, "HWIDChecker.exe");
        }

        public async Task<bool> CheckForUpdatesAsync()
        {
            try
            {
                // Get the latest commit info that modified HWIDChecker.exe
                var latestCommitInfo = await GetLatestCommitForFileAsync();
                if (latestCommitInfo == null)
                {
                    return false;
                }

                // Get current executable's last write time
                var currentFileTime = GetCurrentExecutableTime();
                
                // Debug information (disabled for production - uncomment to troubleshoot)
                /*
                var message = $"Update Check Details:\n\n" +
                             $"Local File Time: {currentFileTime:yyyy-MM-dd HH:mm:ss} UTC (Kind: {currentFileTime.Kind})\n" +
                             $"GitHub Commit Time: {latestCommitInfo.CommitDate:yyyy-MM-dd HH:mm:ss} UTC (Kind: {latestCommitInfo.CommitDate.Kind})\n" +
                             $"GitHub Commit SHA: {latestCommitInfo.Sha[..8]}...\n" +
                             $"Time Difference: {(latestCommitInfo.CommitDate - currentFileTime).TotalMinutes:F1} minutes\n\n";
                */
                
                // Compare times - if GitHub has a newer commit for the exe file, update
                if (latestCommitInfo.CommitDate > currentFileTime)
                {
                    // message += "Result: Update available!";
                    // MessageBox.Show(message, "Update Check Debug", MessageBoxButtons.OK, MessageBoxIcon.Information);
                    return await PerformUpdateAsync(latestCommitInfo.Sha, latestCommitInfo.CommitDate);
                }
                else
                {
                    // message += "Result: No update needed (local version is same or newer)";
                    // MessageBox.Show(message, "Update Check Debug", MessageBoxButtons.OK, MessageBoxIcon.Information);
                    return false;
                }
            }
            catch (Exception ex)
            {
                MessageBox.Show($"Error checking for updates: {ex.Message}", "Update Check Failed",
                               MessageBoxButtons.OK, MessageBoxIcon.Warning);
                return false;
            }
        }

        private async Task<CommitInfo> GetLatestCommitForFileAsync()
        {
            try
            {
                // Get commits that modified HWIDChecker.exe
                var url = $"{GITHUB_API_COMMITS_URL}?path=HWIDChecker.exe&per_page=1";
                var response = await httpClient.GetStringAsync(url);
                using var document = JsonDocument.Parse(response);
                
                if (document.RootElement.GetArrayLength() > 0)
                {
                    var latestCommit = document.RootElement[0];
                    
                    if (latestCommit.TryGetProperty("sha", out var shaElement) &&
                        latestCommit.TryGetProperty("commit", out var commitElement) &&
                        commitElement.TryGetProperty("committer", out var committerElement) &&
                        committerElement.TryGetProperty("date", out var dateElement))
                    {
                        var sha = shaElement.GetString();
                        var dateString = dateElement.GetString();
                        
                        if (sha != null && dateString != null)
                        {
                            // Parse as UTC explicitly to avoid timezone issues
                            if (DateTime.TryParse(dateString, null, System.Globalization.DateTimeStyles.RoundtripKind, out var commitDate))
                            {
                                // Ensure the datetime is treated as UTC
                                if (commitDate.Kind != DateTimeKind.Utc)
                                {
                                    commitDate = DateTime.SpecifyKind(commitDate, DateTimeKind.Utc);
                                }
                                
                                return new CommitInfo
                                {
                                    Sha = sha,
                                    CommitDate = commitDate
                                };
                            }
                        }
                    }
                }
                
                return null;
            }
            catch (Exception ex)
            {
                throw new Exception($"Failed to get latest commit info for HWIDChecker.exe: {ex.Message}");
            }
        }

        private DateTime GetCurrentExecutableTime()
        {
            try
            {
                if (File.Exists(currentExecutablePath))
                {
                    return File.GetLastWriteTimeUtc(currentExecutablePath);
                }
                
                return DateTime.MinValue;
            }
            catch
            {
                return DateTime.MinValue;
            }
        }

        private class CommitInfo
        {
            public string Sha { get; set; } = string.Empty;
            public DateTime CommitDate { get; set; }
        }

        private async Task<bool> PerformUpdateAsync(string newCommitSha, DateTime commitDate)
        {
            try
            {
                // Ask user for confirmation
                var result = MessageBox.Show(
                    "A new version is available. Do you want to update now?\n\n" +
                    "The application will restart after the update.",
                    "Update Available",
                    MessageBoxButtons.YesNo,
                    MessageBoxIcon.Question);

                if (result != DialogResult.Yes)
                {
                    return false;
                }

                // Show progress
                var progressForm = new Form
                {
                    Text = "Updating...",
                    Size = new System.Drawing.Size(300, 100),
                    StartPosition = FormStartPosition.CenterScreen,
                    FormBorderStyle = FormBorderStyle.FixedDialog,
                    MaximizeBox = false,
                    MinimizeBox = false
                };

                var progressLabel = new Label
                {
                    Text = "Downloading update...",
                    Dock = DockStyle.Fill,
                    TextAlign = System.Drawing.ContentAlignment.MiddleCenter
                };

                progressForm.Controls.Add(progressLabel);
                progressForm.Show();
                Application.DoEvents();

                // Download the new executable
                var tempPath = Path.Combine(Path.GetTempPath(), "HWIDChecker_update.exe");
                
                progressLabel.Text = "Downloading new version...";
                Application.DoEvents();
                
                using (var response = await httpClient.GetAsync(GITHUB_RAW_URL))
                {
                    response.EnsureSuccessStatusCode();
                    
                    using (var fileStream = new FileStream(tempPath, FileMode.Create))
                    {
                        await response.Content.CopyToAsync(fileStream);
                    }
                }

                progressLabel.Text = "Preparing to restart...";
                Application.DoEvents();

                // Create batch file for replacement and restart
                var batchPath = Path.Combine(Path.GetTempPath(), "update_hwid_checker.bat");
                var batchContent = $@"
@echo off
timeout /t 2 /nobreak >nul
copy ""{tempPath}"" ""{currentExecutablePath}"" /Y
del ""{tempPath}""
start """" ""{currentExecutablePath}""
del ""{batchPath}""
";

                File.WriteAllText(batchPath, batchContent);

                progressForm.Close();

                // Start the batch file and exit current application
                var processInfo = new ProcessStartInfo
                {
                    FileName = batchPath,
                    UseShellExecute = false,
                    CreateNoWindow = true
                };

                Process.Start(processInfo);
                
                // Exit current application
                Application.Exit();
                Environment.Exit(0);

                return true;
            }
            catch (Exception ex)
            {
                MessageBox.Show($"Update failed: {ex.Message}", "Update Error", 
                               MessageBoxButtons.OK, MessageBoxIcon.Error);
                return false;
            }
        }

        public void Dispose()
        {
            httpClient?.Dispose();
        }
    }
}