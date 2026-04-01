using System;
using System.Windows.Forms;
using System.Drawing;
using System.Collections.Generic;
using HWIDChecker.Services;
using HWIDChecker.Services.Models;
using HWIDChecker.UI.Components;
using System.Threading.Tasks;

namespace HWIDChecker.UI.Forms
{
    public class CleanDevicesForm : Form
    {
        private const int DefaultWidth = 920;
        private const int DefaultHeight = 640;
        private const int MinimumWidth = 760;
        private const int MinimumHeight = 500;
        private TextBox outputTextBox;
        private Button closeButton;
        private Button recleanButton;
        private Button whitelistButton;
        private FlowLayoutPanel actionButtonPanel;
        private SystemCleaningService cleaningService;
        private DeviceWhitelistService whitelistService;
        private List<DeviceDetail> ghostDevices = null;
        private bool isProcessing;

        public CleanDevicesForm()
        {
            InitializeComponents();
            this.cleaningService = new SystemCleaningService();
            this.whitelistService = new DeviceWhitelistService();
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
            this.Text = "Device Cleaning";
            
            // Use native .NET DPI handling
            this.FormBorderStyle = FormBorderStyle.Sizable;
            this.StartPosition = FormStartPosition.CenterParent;
            this.MaximizeBox = true;
            this.MinimizeBox = false;
            this.AutoScaleMode = AutoScaleMode.Dpi;
            this.ClientSize = new Size(DefaultWidth, DefaultHeight);
            this.MinimumSize = new Size(MinimumWidth, MinimumHeight);
            this.BackColor = ThemeColors.MainBackground;
            this.ForeColor = ThemeColors.PrimaryText;

            // Subscribe to DPI changes for runtime handling
            this.DpiChanged += CleanDevicesForm_DpiChanged;

            outputTextBox = new TextBox
            {
                Multiline = true,
                ReadOnly = true,
                ScrollBars = ScrollBars.Both,
                WordWrap = false,
                Dock = DockStyle.Fill,
                BackColor = ThemeColors.TextBoxBackground,
                ForeColor = ThemeColors.TextBoxText,
                Font = new Font("Consolas", 9.75f, FontStyle.Regular)
            };
            
            whitelistButton = CreateActionButton("Manage Whitelist", Buttons.ButtonVariant.Primary);
            whitelistButton.Enabled = false;
            whitelistButton.Click += WhitelistButton_Click;

            recleanButton = CreateActionButton("Reclean", Buttons.ButtonVariant.Primary);
            recleanButton.Enabled = false;
            recleanButton.Click += (s, e) => StartCleaningProcess();

            closeButton = CreateActionButton("Close", Buttons.ButtonVariant.Primary);
            closeButton.Enabled = false;
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

            // Output container
            var outputPanel = new Panel
            {
                Dock = DockStyle.Fill,
                Padding = new Padding(10),
                BackColor = ThemeColors.MainBackground
            };
            outputPanel.Controls.Add(outputTextBox);

            // Bottom action bar
            actionButtonPanel = new FlowLayoutPanel
            {
                Dock = DockStyle.Fill,
                FlowDirection = FlowDirection.RightToLeft,
                WrapContents = false,
                Padding = new Padding(10, 10, 10, 10),
                BackColor = ThemeColors.ButtonPanelBackground
            };
            actionButtonPanel.Controls.Add(closeButton);
            actionButtonPanel.Controls.Add(recleanButton);
            actionButtonPanel.Controls.Add(whitelistButton);

            // Root layout
            var mainLayout = new TableLayoutPanel
            {
                Dock = DockStyle.Fill,
                ColumnCount = 1,
                RowCount = 2,
                BackColor = ThemeColors.MainBackground
            };
            mainLayout.ColumnStyles.Add(new ColumnStyle(SizeType.Percent, 100F));
            mainLayout.RowStyles.Add(new RowStyle(SizeType.Percent, 100F));
            mainLayout.RowStyles.Add(new RowStyle(SizeType.Absolute, 56F));
            mainLayout.Controls.Add(outputPanel, 0, 0);
            mainLayout.Controls.Add(actionButtonPanel, 0, 1);

            this.Controls.Add(mainLayout);

            this.Load += CleanDevicesForm_Load;
        }

        private Button CreateActionButton(string text, Buttons.ButtonVariant variant)
        {
            var button = new Button
            {
                Text = text,
                AutoSize = true,
                AutoSizeMode = AutoSizeMode.GrowAndShrink,
                MinimumSize = new Size(130, 34),
                Padding = new Padding(12, 4, 12, 4),
                Margin = new Padding(0, 0, 8, 0),
                UseVisualStyleBackColor = true
            };

            Buttons.ApplyStyle(button, variant);
            return button;
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
                whitelistButton.Enabled = false;
                closeButton.Enabled = false;
                outputTextBox.Clear();
                ghostDevices = null;

                // Scan for ghost devices
                HandleStatusUpdate("Scanning for non-present (ghost) devices...\r\n");
                ghostDevices = await cleaningService.ScanForGhostDevicesAsync();

                if (ghostDevices.Count > 0)
                {
                    // Filter out whitelisted devices
                    var nonWhitelistedDevices = new List<DeviceDetail>();
                    foreach (var device in ghostDevices)
                    {
                        if (!whitelistService.IsDeviceWhitelisted(device))
                        {
                            nonWhitelistedDevices.Add(device);
                        }
                    }

                    HandleStatusUpdate("The following non-present (ghost) devices were found:");
                    foreach (var device in ghostDevices)
                    {
                        HandleStatusUpdate("----------------------------------------");
                        HandleStatusUpdate($"Device Name       : {device.Name}");
                        HandleStatusUpdate($"Device Description: {device.Description}");
                        HandleStatusUpdate($"Hardware ID       : {device.HardwareId}");
                        HandleStatusUpdate($"Class             : {device.Class}");
                        if (whitelistService.IsDeviceWhitelisted(device))
                        {
                            HandleStatusUpdate("Status            : Whitelisted (will not be removed)");
                        }
                    }
                    HandleStatusUpdate("----------------------------------------");
                    HandleStatusUpdate($"Total non-present devices found: {ghostDevices.Count}");
                    HandleStatusUpdate($"Whitelisted devices: {ghostDevices.Count - nonWhitelistedDevices.Count}");
                    HandleStatusUpdate($"Devices that can be removed: {nonWhitelistedDevices.Count}");

                    if (nonWhitelistedDevices.Count > 0)
                    {
                        var result = DeviceRemovalConfirmationForm.ShowConfirmation(this, nonWhitelistedDevices.Count);

                        if (result == DeviceRemovalConfirmationForm.ConfirmationResult.YesAutoClose)
                        {
                            await cleaningService.RemoveGhostDevicesAsync(nonWhitelistedDevices);
                            HandleStatusUpdate("\r\nDevice cleaning process completed.");
                            
                            // Auto-close after 1 second
                            var closeTimer = new System.Windows.Forms.Timer { Interval = 1000 };
                            closeTimer.Tick += (s, e) =>
                            {
                                closeTimer.Stop();
                                closeTimer.Dispose();
                                this.Close();
                            };
                            closeTimer.Start();
                            return; // Exit early to avoid the completion message below
                        }
                        else if (result == DeviceRemovalConfirmationForm.ConfirmationResult.Yes)
                        {
                            await cleaningService.RemoveGhostDevicesAsync(nonWhitelistedDevices);
                        }
                        else
                        {
                            HandleStatusUpdate("\r\nOperation cancelled. No devices were removed.");
                        }
                    }
                    else
                    {
                        HandleStatusUpdate("\r\nAll devices are whitelisted. No devices need to be cleaned.");
                        
                        // Auto-close after 1 second when all devices are whitelisted
                        var closeTimer = new System.Windows.Forms.Timer { Interval = 1000 };
                        closeTimer.Tick += (s, e) =>
                        {
                            closeTimer.Stop();
                            closeTimer.Dispose();
                            this.Close();
                        };
                        closeTimer.Start();
                        return; // Exit early to avoid showing completion message
                    }
                }
                else
                {
                    HandleStatusUpdate("No non-present devices were found.");
                    
                    // Auto-close after 1 second when no devices found
                    var closeTimer = new System.Windows.Forms.Timer { Interval = 1000 };
                    closeTimer.Tick += (s, e) =>
                    {
                        closeTimer.Stop();
                        closeTimer.Dispose();
                        this.Close();
                    };
                    closeTimer.Start();
                    return; // Exit early to avoid showing completion message
                }

                HandleStatusUpdate("\r\nDevice cleaning process completed.");
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
                whitelistButton.Enabled = ghostDevices != null && ghostDevices.Count > 0;
                closeButton.Enabled = true;
            }
        }

        private bool IsAdministrator()
        {
            var identity = System.Security.Principal.WindowsIdentity.GetCurrent();
            var principal = new System.Security.Principal.WindowsPrincipal(identity);
            return principal.IsInRole(System.Security.Principal.WindowsBuiltInRole.Administrator);
        }

        private void WhitelistButton_Click(object sender, EventArgs e)
        {
            if (ghostDevices != null && ghostDevices.Count > 0)
            {
                using (var whitelistForm = new WhitelistDevicesForm(ghostDevices))
                {
                    if (whitelistForm.ShowDialog() == DialogResult.OK)
                    {
                        HandleStatusUpdate("\r\nDevice whitelist has been updated.");
                    }
                }
            }
            else
            {
                MessageBox.Show(
                    "No ghost devices found to whitelist. Please scan for devices first.",
                    "No Devices Found",
                    MessageBoxButtons.OK,
                    MessageBoxIcon.Information);
            }
        }

        private void CleanDevicesForm_DpiChanged(object sender, DpiChangedEventArgs e)
        {
            // Handle DPI changes at runtime
            this.PerformAutoScale();
            this.Invalidate();
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
