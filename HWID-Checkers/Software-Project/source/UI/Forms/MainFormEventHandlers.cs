using System;
using System.Threading.Tasks;
using System.Windows.Forms;
using HWIDChecker.Forms;
using HWIDChecker.Services;
using HWIDChecker.UI.Components;
using HWIDChecker.Hardware;

namespace HWIDChecker.UI.Forms
{
    public class MainFormEventHandlers
    {
        private readonly MainForm mainForm;
        private readonly MainFormLayout layout;
        private readonly FileExportService fileExportService;
        private readonly HardwareInfoManager hardwareInfoManager;
        public MainFormEventHandlers(MainForm mainForm, MainFormLayout layout, FileExportService fileExportService, HardwareInfoManager hardwareInfoManager)
        {
            this.mainForm = mainForm;
            this.layout = layout;
            this.fileExportService = fileExportService;
            this.hardwareInfoManager = hardwareInfoManager;
        }

        public void InitializeEventHandlers(Func<Task> loadHardwareInfo)
        {
            layout.RefreshButton.Click += async (s, e) => await loadHardwareInfo();
            layout.ExportButton.Click += ExportButton_Click;
            layout.CleanDevicesButton.Click += CleanDevicesButton_Click;
            layout.CleanLogsButton.Click += CleanLogsButton_Click;
        }


        private void ExportButton_Click(object sender, EventArgs e)
        {
            try
            {
                var filePath = fileExportService.ExportHardwareInfo(layout.OutputTextBox.Text);
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

        private bool IsAdministrator()
        {
            var identity = System.Security.Principal.WindowsIdentity.GetCurrent();
            var principal = new System.Security.Principal.WindowsPrincipal(identity);
            return principal.IsInRole(System.Security.Principal.WindowsBuiltInRole.Administrator);
        }
    }
}