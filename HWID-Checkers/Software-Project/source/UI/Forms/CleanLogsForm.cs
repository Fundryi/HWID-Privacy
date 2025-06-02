using System;
using System.Windows.Forms;
using System.Drawing;
using HWIDChecker.Services;
using System.Threading.Tasks;

namespace HWIDChecker.UI.Forms
{
    public class CleanLogsForm : Form
    {
        private TextBox outputTextBox;
        private SystemCleaningService cleaningService;
        private bool isProcessing;

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
            this.Width = 600;
            this.Height = 400;
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

            // Create panel for outputTextBox
            var panel = new Panel
            {
                Dock = DockStyle.Fill,
                Padding = new Padding(10)
            };
            panel.Controls.Add(outputTextBox);

            this.Controls.Add(panel);

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
                outputTextBox.Clear();

                // Clean event logs only
                await cleaningService.CleanLogsAsync();

                HandleStatusUpdate("\r\nLog cleaning process completed.");
                
                // Auto-close after 1 second
                var closeTimer = new System.Windows.Forms.Timer { Interval = 1000 };
                closeTimer.Tick += (s, e) =>
                {
                    closeTimer.Stop();
                    closeTimer.Dispose();
                    this.Close();
                };
                closeTimer.Start();
            }
            catch (Exception ex)
            {
                HandleError("Log Cleaning Process", ex.Message);
                MessageBox.Show($"Error during log cleaning process: {ex.Message}",
                    "Error", MessageBoxButtons.OK, MessageBoxIcon.Error);
                
                // Close after error as well
                this.Close();
            }
            finally
            {
                isProcessing = false;
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
                // Allow closing even during processing for logs-only operation
                isProcessing = false;
            }
            base.OnFormClosing(e);
        }
    }
}