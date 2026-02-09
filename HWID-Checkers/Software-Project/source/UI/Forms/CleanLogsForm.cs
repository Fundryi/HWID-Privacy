using System;
using System.Windows.Forms;
using System.Drawing;
using System.Threading;
using HWIDChecker.Services;
using HWIDChecker.UI.Components;
using System.Threading.Tasks;

namespace HWIDChecker.UI.Forms
{
    public class CleanLogsForm : Form
    {
        private const int DefaultWidth = 760;
        private const int DefaultHeight = 460;
        private const int MinimumWidth = 620;
        private const int MinimumHeight = 380;
        private TextBox outputTextBox;
        private Button closeButton;
        private SystemCleaningService cleaningService;
        private CancellationTokenSource cleaningCancellationTokenSource;
        private bool isProcessing;
        private bool forceClosingRequested;

        public CleanLogsForm()
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
            this.Text = "Log Cleaning";
            this.StartPosition = FormStartPosition.CenterParent;
            this.FormBorderStyle = FormBorderStyle.Sizable;
            this.MaximizeBox = true;
            this.MinimizeBox = false;
            this.AutoScaleMode = AutoScaleMode.Dpi;
            this.ClientSize = new Size(DefaultWidth, DefaultHeight);
            this.MinimumSize = new Size(MinimumWidth, MinimumHeight);
            this.BackColor = ThemeColors.MainBackground;
            this.ForeColor = ThemeColors.PrimaryText;
            this.DpiChanged += CleanLogsForm_DpiChanged;

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

            closeButton = new Button
            {
                Text = "Close",
                AutoSize = true,
                AutoSizeMode = AutoSizeMode.GrowAndShrink,
                MinimumSize = new Size(110, 34),
                Padding = new Padding(12, 4, 12, 4),
                Margin = new Padding(0, 0, 8, 0),
                Enabled = true
            };
            Buttons.ApplyStyle(closeButton, Buttons.ButtonVariant.Primary);
            closeButton.Click += CloseButton_Click;

            // Output container
            var outputPanel = new Panel
            {
                Dock = DockStyle.Fill,
                Padding = new Padding(10),
                BackColor = ThemeColors.MainBackground
            };
            outputPanel.Controls.Add(outputTextBox);

            // Bottom action bar
            var actionButtonPanel = new FlowLayoutPanel
            {
                Dock = DockStyle.Fill,
                FlowDirection = FlowDirection.RightToLeft,
                WrapContents = false,
                Padding = new Padding(10, 10, 10, 10),
                BackColor = ThemeColors.ButtonPanelBackground
            };
            actionButtonPanel.Controls.Add(closeButton);

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

            this.Load += CleanLogsForm_Load;
        }

        private void CleanLogsForm_Load(object sender, EventArgs e)
        {
            if (!IsAdministrator())
            {
                MessageBox.Show("This operation requires administrative privileges. Please run the application as administrator.",
                    "Administrator Rights Required", MessageBoxButtons.OK, MessageBoxIcon.Warning);
                this.Close();
                return;
            }

            StartLogCleaningProcess();
        }

        private async void StartLogCleaningProcess()
        {
            try
            {
                isProcessing = true;
                forceClosingRequested = false;
                closeButton.Enabled = true;
                closeButton.Text = "Stop & Close";
                outputTextBox.Clear();

                cleaningCancellationTokenSource?.Dispose();
                cleaningCancellationTokenSource = new CancellationTokenSource();

                HandleStatusUpdate("Starting event log cleanup...");
                HandleStatusUpdate("Enumerating and clearing logs. This may take a moment.\r\n");

                // Clean event logs only
                await cleaningService.CleanLogsAsync(cleaningCancellationTokenSource.Token);

                HandleStatusUpdate("\r\nLog cleaning process completed.");
                HandleStatusUpdate("Review the summary above. This window will stay open.");
            }
            catch (OperationCanceledException)
            {
                HandleStatusUpdate("\r\nLog cleaning canceled by user.");
            }
            catch (Exception ex)
            {
                HandleError("Log Cleaning Process", ex.Message);
                MessageBox.Show($"Error during log cleaning process: {ex.Message}",
                    "Error", MessageBoxButtons.OK, MessageBoxIcon.Error);

                HandleStatusUpdate("Log cleaning encountered an error. Review details above.");
            }
            finally
            {
                isProcessing = false;
                cleaningCancellationTokenSource?.Dispose();
                cleaningCancellationTokenSource = null;
                if (!this.IsDisposed)
                {
                    closeButton.Enabled = true;
                    closeButton.Text = "Close";
                }
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
            if (isProcessing)
            {
                if (!forceClosingRequested)
                {
                    var result = MessageBox.Show(
                        "Log cleaning is still running. Force-stop and close this window?",
                        "Confirm Stop",
                        MessageBoxButtons.YesNo,
                        MessageBoxIcon.Warning);

                    if (result != DialogResult.Yes)
                    {
                        e.Cancel = true;
                        return;
                    }
                }

                forceClosingRequested = true;
                cleaningCancellationTokenSource?.Cancel();
            }
            base.OnFormClosing(e);
        }

        private void CloseButton_Click(object sender, EventArgs e)
        {
            if (!isProcessing)
            {
                this.Close();
                return;
            }

            var result = MessageBox.Show(
                "Log cleaning is still running. Force-stop and close this window?",
                "Confirm Stop",
                MessageBoxButtons.YesNo,
                MessageBoxIcon.Warning);

            if (result != DialogResult.Yes)
            {
                return;
            }

            forceClosingRequested = true;
            cleaningCancellationTokenSource?.Cancel();
            this.Close();
        }

        private void CleanLogsForm_DpiChanged(object sender, DpiChangedEventArgs e)
        {
            this.PerformAutoScale();
            this.Invalidate();
        }
    }
}
