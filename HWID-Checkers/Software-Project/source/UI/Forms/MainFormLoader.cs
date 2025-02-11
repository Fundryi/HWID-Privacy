using System;
using System.Windows.Forms;
using System.Threading.Tasks;
using HWIDChecker.Hardware;
using HWIDChecker.UI.Components;

namespace HWIDChecker.UI.Forms
{
    public class MainFormLoader
    {
        private readonly MainFormLayout layout;
        private readonly HardwareInfoManager hardwareInfoManager;
        private readonly Form parentForm;

        public MainFormLoader(Form parentForm, MainFormLayout layout, HardwareInfoManager hardwareInfoManager)
        {
            this.parentForm = parentForm;
            this.layout = layout;
            this.hardwareInfoManager = hardwareInfoManager;
        }

        public async Task LoadHardwareInfo()
        {
            try
            {
                SetLoadingState(true);
                layout.OutputTextBox.Text = string.Empty;

                // Initialize array to store results in order
                var orderedResults = new string[hardwareInfoManager.GetProviderCount()];

                // Create progress handler that maintains order
                // Disable scrolling during loading
                var originalScrollBars = layout.OutputTextBox.ScrollBars;
                layout.OutputTextBox.ScrollBars = ScrollBars.None;

                var progress = new Progress<(int index, string content)>(update =>
                {
                    // Store result at correct index and immediately show
                    orderedResults[update.index] = update.content;

                    // Update text with all available results
                    var currentResults = orderedResults.Where(r => r != null);
                    layout.OutputTextBox.Text = string.Join(string.Empty, currentResults);

                    // Always scroll to top during loading
                    layout.OutputTextBox.SelectionStart = 0;
                    layout.OutputTextBox.ScrollToCaret();
                });

                // Get hardware info with ordered progress updates
                var result = await hardwareInfoManager.GetAllHardwareInfo(progress);

                SetLoadingState(false);
            }
            catch (Exception ex)
            {
                MessageBox.Show($"Error loading HWID data: {ex.Message}", "Error",
                    MessageBoxButtons.OK, MessageBoxIcon.Error);
                SetLoadingState(false);
            }
        }

        private void SetLoadingState(bool isLoading)
        {
            layout.RefreshButton.Enabled = !isLoading;
            layout.ExportButton.Enabled = !isLoading;
            layout.CompareButton.Enabled = !isLoading;
            layout.LoadingLabel.Visible = isLoading;

            if (isLoading)
            {
                layout.LoadingLabel.BringToFront();
                layout.OutputTextBox.Text = string.Empty;
                layout.OutputTextBox.ScrollBars = ScrollBars.None; // Disable scrolling during load
                Application.DoEvents();
            }
            else
            {
                layout.OutputTextBox.ScrollBars = ScrollBars.Vertical; // Restore scrolling when done
            }
        }
    }
}