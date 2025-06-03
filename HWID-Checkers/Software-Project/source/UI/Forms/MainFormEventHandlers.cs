using System;
using System.Threading.Tasks;
using System.Windows.Forms;
using HWIDChecker.Forms;
using HWIDChecker.Services;
using HWIDChecker.UI.Components;
using HWIDChecker.Hardware;
using static HWIDChecker.Services.UpdateResult;

namespace HWIDChecker.UI.Forms
{
    public class MainFormEventHandlers
    {
        private readonly MainForm mainForm;
        private readonly MainFormLayout layout;
        private readonly FileExportService fileExportService;
        private readonly HardwareInfoManager hardwareInfoManager;
        private readonly AutoUpdateService autoUpdateService;
        
        public MainFormEventHandlers(MainForm mainForm, MainFormLayout layout, FileExportService fileExportService, HardwareInfoManager hardwareInfoManager, AutoUpdateService autoUpdateService)
        {
            this.mainForm = mainForm;
            this.layout = layout;
            this.fileExportService = fileExportService;
            this.hardwareInfoManager = hardwareInfoManager;
            this.autoUpdateService = autoUpdateService;
        }

        public void InitializeEventHandlers(Func<Task> loadHardwareInfo)
        {
            layout.RefreshButton.Click += async (s, e) => await loadHardwareInfo();
            layout.ExportButton.Click += ExportButton_Click;
            layout.CleanDevicesButton.Click += CleanDevicesButton_Click;
            layout.CleanLogsButton.Click += CleanLogsButton_Click;
            layout.CheckUpdatesButton.Click += CheckUpdatesButton_Click;
            layout.SectionedViewButton.Click += SectionedViewButton_Click;
        }


        private void ExportButton_Click(object sender, EventArgs e)
        {
            try
            {
                // Use the helper method to ensure we get the complete content for export
                // This maintains the original formatting with separators for exported files
                var contentToExport = layout.GetAllContentForExport();
                var filePath = fileExportService.ExportHardwareInfo(contentToExport);
                MessageBox.Show($"Export completed successfully!\nSaved to: {filePath}",
                    "Export", MessageBoxButtons.OK, MessageBoxIcon.Information);
            }
            catch (Exception ex)
            {
                MessageBox.Show($"Error exporting file: {ex.Message}",
                    "Export Error", MessageBoxButtons.OK, MessageBoxIcon.Error);
            }
        }

        private void CleanDevicesButton_Click(object sender, EventArgs e)
        {
            var cleanForm = new CleanDevicesForm();
            cleanForm.ShowDialog(mainForm);
        }

        private void CleanLogsButton_Click(object sender, EventArgs e)
        {
            var cleanLogsForm = new CleanLogsForm();
            cleanLogsForm.ShowDialog(mainForm);
        }

        private void SectionedViewButton_Click(object sender, EventArgs e)
        {
            // Pass the existing data from the main form to avoid re-loading
            var existingData = layout.GetAllContentForExport();
            var sectionedViewForm = new SectionedViewForm(hardwareInfoManager, existingData);
            sectionedViewForm.ShowDialog(mainForm);
        }

        private async void CheckUpdatesButton_Click(object sender, EventArgs e)
        {
            var button = sender as Button;
            if (button != null)
            {
                button.Enabled = false;
                button.Text = "Checking...";
            }

            try
            {
                var updateResult = await autoUpdateService.CheckForUpdatesAsync();
                
                switch (updateResult)
                {
                    case UpdateResult.NoUpdateAvailable:
                        MessageBox.Show("You are already running the latest version.", "No Updates Available",
                                       MessageBoxButtons.OK, MessageBoxIcon.Information);
                        break;
                        
                    case UpdateResult.UserDeclined:
                        // User chose not to update - don't show any message
                        break;
                        
                    case UpdateResult.UpdateCompleted:
                        // App will restart automatically - this case shouldn't be reached
                        break;
                        
                    case UpdateResult.Error:
                        // Error message already shown in AutoUpdateService
                        break;
                }
            }
            catch (Exception ex)
            {
                MessageBox.Show($"Error checking for updates: {ex.Message}", "Update Check Failed",
                               MessageBoxButtons.OK, MessageBoxIcon.Warning);
            }
            finally
            {
                if (button != null)
                {
                    button.Enabled = true;
                    button.Text = "Check Updates";
                }
            }
        }

        private bool IsAdministrator()
        {
            var identity = System.Security.Principal.WindowsIdentity.GetCurrent();
            var principal = new System.Security.Principal.WindowsPrincipal(identity);
            return principal.IsInRole(System.Security.Principal.WindowsBuiltInRole.Administrator);
        }
    }
}