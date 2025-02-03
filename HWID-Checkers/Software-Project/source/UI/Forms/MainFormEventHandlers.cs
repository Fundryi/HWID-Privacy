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
            layout.CompareButton.Click += CompareButton_Click;
        }

        private void CompareButton_Click(object sender, EventArgs e)
        {
            try
            {
                var exportFiles = Directory.GetFiles(Application.StartupPath, "HWID-EXPORT-*.txt")
                    .OrderByDescending(f => f)
                    .ToList();

                if (exportFiles.Count == 0)
                {
                    MessageBox.Show("No exported configurations found. Please export your current configuration first.",
                        "Compare", MessageBoxButtons.OK, MessageBoxIcon.Information);
                    return;
                }

                var compareForm = CompareForm.CreateCompareWithCurrent(layout.OutputTextBox.Text, exportFiles);
                if (compareForm != null)
                {
                    // Get the screen that contains the main form
                    Screen screen = Screen.FromControl(mainForm);
                    var screenBounds = screen.WorkingArea;

                    // Initialize form properties before showing
                    compareForm.StartPosition = FormStartPosition.Manual;
                    compareForm.Opacity = 0;
                    compareForm.Location = new Point(
                        screenBounds.Left + ((screenBounds.Width - compareForm.Width) / 2),
                        screenBounds.Top + ((screenBounds.Height - compareForm.Height) / 2)
                    );

                    // Setup form closing handler before showing
                    compareForm.FormClosed += (s, args) => mainForm.Show();

                    // Hide main form and show compare form
                    mainForm.Hide();
                    compareForm.Show();

                    // Create smooth fade-in effect
                    var fadeTimer = new System.Windows.Forms.Timer { Interval = 10 };
                    fadeTimer.Tick += (s, e) =>
                    {
                        if (compareForm.Opacity < 1)
                        {
                            compareForm.Opacity += 0.1;
                        }
                        else
                        {
                            fadeTimer.Stop();
                            fadeTimer.Dispose();
                        }
                    };
                    fadeTimer.Start();
                }
            }
            catch (Exception ex)
            {
                MessageBox.Show($"Error opening compare window: {ex.Message}",
                    "Compare Error", MessageBoxButtons.OK, MessageBoxIcon.Error);
            }
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
    }
}