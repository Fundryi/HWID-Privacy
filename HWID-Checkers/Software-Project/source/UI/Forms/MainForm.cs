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
            
            // Re-scale the form and all controls
            SuspendLayout();
            try
            {
                dpiService.ScaleControl(this);
                initializer.Layout?.UpdateLoadingLabelPosition(this);
            }
            finally
            {
                ResumeLayout(true);
            }
        }

        protected override void SetBoundsCore(int x, int y, int width, int height, BoundsSpecified specified)
        {
            // Apply DPI scaling to form bounds
            if (dpiService != null && dpiService.IsHighDpi)
            {
                var scaledSize = dpiService.ScaleSize(new System.Drawing.Size(width, height));
                base.SetBoundsCore(x, y, scaledSize.Width, scaledSize.Height, specified);
            }
            else
            {
                base.SetBoundsCore(x, y, width, height, specified);
            }
        }
    }
}