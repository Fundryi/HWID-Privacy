using System.Windows.Forms;
using HWIDChecker.UI.Forms;
using HWIDChecker.Services;

namespace HWIDChecker.UI.Forms
{
    public partial class MainForm : Form
    {
        private readonly MainFormInitializer initializer;
        private readonly DpiScalingService dpiService;

        public MainForm()
        {
            // Initialize DPI scaling before any UI operations
            dpiService = DpiScalingService.Instance;
            
            initializer = new MainFormInitializer(this);
            initializer.Initialize();
            
            // Handle DPI changes at runtime
            this.DpiChanged += MainForm_DpiChanged;
        }

        private void MainForm_DpiChanged(object sender, DpiChangedEventArgs e)
        {
            // Update DPI scaling service with new DPI
            dpiService.UpdateScaleFactorForWindow(this.Handle);
            
            // Only update loading label position - don't re-scale all controls
            // to prevent the form from becoming unusable
            initializer.Layout?.UpdateLoadingLabelPosition(this);
        }
    }
}