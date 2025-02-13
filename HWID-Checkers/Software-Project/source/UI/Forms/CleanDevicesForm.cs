using System;
using System.Windows.Forms;
using System.Drawing;
using System.Collections.Generic;
using HWIDChecker.Services;
using System.Threading.Tasks;

namespace HWIDChecker.UI.Forms
{
    public class CleanDevicesForm : Form
    {
        private TextBox outputTextBox;
        private Button closeButton;
        private Button recleanButton;
        private SystemCleaningService cleaningService;
        private List<SystemCleaningService.DeviceDetail> ghostDevices;
        private bool isProcessing;

        public CleanDevicesForm()
        {
            InitializeComponents();
            this.cleaningService = new SystemCleaningService();
            this.cleaningService.OnStatusUpdate += HandleStatusUpdate;
            this.cleaningService.OnError += HandleError;
        }

        private void HandleStatusUpdate(string message)
        {
            if (this.IsDisposed) return;

            if (InvokeRequired)
            {
                BeginInvoke(new Action(() => HandleStatusUpdate(message)));
                return;
            }

            outputTextBox.AppendText(message + "\r\n");
            outputTextBox.SelectionStart = outputTextBox.TextLength;
            outputTextBox.ScrollToCaret();
            Application.DoEvents();
        }

        private void HandleError(string source, string error)
        {
            if (this.IsDisposed) return;

            if (InvokeRequired)
            {
                BeginInvoke(new Action(() => HandleError(source, error)));
                return;
            }

            outputTextBox.AppendText($"Error in {source}: {error}\r\n");
            outputTextBox.SelectionStart = outputTextBox.TextLength;
            outputTextBox.ScrollToCaret();
            Application.DoEvents();
        }

        private void InitializeComponents()
        {
            this.Text = "System Cleaning";
            this.Width = 800;
            this.Height = 600;
            this.StartPosition = FormStartPosition.CenterScreen;
            this.FormBorderStyle = FormBorderStyle.FixedDialog;
            this.MaximizeBox = false;
            this.MinimizeBox = false;

            outputTextBox = new TextBox
            {
                Multiline = true,
                ReadOnly = true,
                ScrollBars = ScrollBars.Vertical,
                Dock = DockStyle.Fill,
                BackColor = Color.Black,
                ForeColor = Color.Lime,
                Font = new Font("Consolas", 9.75f, FontStyle.Regular)
            };
recleanButton = new Button
{
    Text = "Reclean",
    Dock = DockStyle.Bottom,
    Height = 30,
    Enabled = false
};
recleanButton.Click += (s, e) => StartCleaningProcess();

closeButton = new Button
{
    Text = "Close",
    Dock = DockStyle.Bottom,
    Height = 30,
    Enabled = false
};
closeButton.Click += (s, e) =>
            closeButton.Click += (s, e) => 
            {
                if (isProcessing)
                {
                    if (MessageBox.Show("Operation in progress. Are you sure you want to close?", 
                        "Confirm Exit", MessageBoxButtons.YesNo, MessageBoxIcon.Warning) != DialogResult.Yes)
                    {
                        return;
                    }
                }
                this.Close();
            };

            // Create panel for outputTextBox
            var panel = new Panel
            {
                Dock = DockStyle.Fill,
                Padding = new Padding(10)
            };
            panel.Controls.Add(outputTextBox);

            this.Controls.Add(panel);
            this.Controls.Add(recleanButton);
            this.Controls.Add(closeButton);

            this.Load += CleanDevicesForm_Load;
        }

        private void CleanDevicesForm_Load(object sender, EventArgs e)
        {
            if (!IsAdministrator())
            {
                MessageBox.Show("This operation requires administrative privileges. Please run the application as administrator.",
                    "Administrator Rights Required", MessageBoxButtons.OK, MessageBoxIcon.Warning);
                this.Close();
                return;
            }

            StartCleaningProcess();
        }

        private async void StartCleaningProcess()
        {
            try
            {
                isProcessing = true;
                recleanButton.Enabled = false;
                closeButton.Enabled = false;
                outputTextBox.Clear();

                // First clean event logs
                await cleaningService.CleanLogsAsync();

                // Then scan for ghost devices
                HandleStatusUpdate("\r\nScanning for non-present (ghost) devices...\r\n");
                var devices = await cleaningService.ScanForGhostDevicesAsync();

                if (devices.Count > 0)
                {
                    HandleStatusUpdate("The following non-present (ghost) devices were found:");
                    foreach (var device in devices)
                    {
                        HandleStatusUpdate("----------------------------------------");
                        HandleStatusUpdate($"Device Name       : {device.Name}");
                        HandleStatusUpdate($"Device Description: {device.Description}");
                        HandleStatusUpdate($"Hardware ID       : {device.HardwareId}");
                        HandleStatusUpdate($"Class             : {device.Class}");
                    }
                    HandleStatusUpdate("----------------------------------------");
                    HandleStatusUpdate($"Total non-present devices found: {devices.Count}");

                    var result = MessageBox.Show(
                        $"Do you want to remove these {devices.Count} ghost devices?\n\n" +
                        "Warning: Device removal cannot be undone.",
                        "Confirm Device Removal",
                        MessageBoxButtons.YesNo,
                        MessageBoxIcon.Warning);

                    if (result == DialogResult.Yes)
                    {
                        await cleaningService.RemoveGhostDevicesAsync(devices);
                    }
                    else
                    {
                        HandleStatusUpdate("\r\nOperation cancelled. No devices were removed.");
                    }
                }
                else
                {
                    HandleStatusUpdate("No non-present devices were found.");
                }

                HandleStatusUpdate("\r\nSystem cleaning process completed.");
            }
            catch (Exception ex)
            {
                HandleError("Cleaning Process", ex.Message);
                MessageBox.Show($"Error during cleaning process: {ex.Message}",
                    "Error", MessageBoxButtons.OK, MessageBoxIcon.Error);
            }
            finally
            {
                isProcessing = false;
                recleanButton.Enabled = true;
                closeButton.Enabled = true;
            }
        }

        private bool IsAdministrator()
        {
            var identity = System.Security.Principal.WindowsIdentity.GetCurrent();
            var principal = new System.Security.Principal.WindowsPrincipal(identity);
            return principal.IsInRole(System.Security.Principal.WindowsBuiltInRole.Administrator);
        }

        protected override void OnFormClosing(FormClosingEventArgs e)
        {
            base.OnFormClosing(e);
            if (ghostDevices != null)
            {
                foreach (var device in ghostDevices)
                {
                    // No manual cleanup needed as Windows handles the cleanup of the device info set
                }
            }
        }
    }
}