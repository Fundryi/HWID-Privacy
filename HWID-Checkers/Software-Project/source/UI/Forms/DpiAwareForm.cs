using System;
using System.Drawing;
using System.Windows.Forms;
using HWIDChecker.Services;

namespace HWIDChecker.UI.Forms
{
    /// <summary>
    /// Base form class that provides DPI scaling functionality for all dialog forms
    /// </summary>
    public class DpiAwareForm : Form
    {
        protected readonly DpiScalingService dpiService;

        public DpiAwareForm()
        {
            // Initialize DPI scaling before any UI operations
            dpiService = DpiScalingService.Instance;
            
            // Enable automatic scaling for the form
            AutoScaleMode = AutoScaleMode.Dpi;
            
            // Handle DPI changes at runtime
            this.DpiChanged += DpiAwareForm_DpiChanged;
        }

        private void DpiAwareForm_DpiChanged(object sender, DpiChangedEventArgs e)
        {
            // Update DPI scaling service with new DPI
            dpiService.UpdateScaleFactorForWindow(this.Handle);
            
            // Re-scale the form and all controls
            SuspendLayout();
            try
            {
                dpiService.ScaleControl(this);
            }
            finally
            {
                ResumeLayout(true);
            }
        }

        /// <summary>
        /// Apply DPI scaling to the form after all controls have been added
        /// </summary>
        protected void ApplyDpiScaling()
        {
            dpiService.ScaleControl(this);
        }

        /// <summary>
        /// Helper method to create a DPI-scaled font
        /// </summary>
        protected Font CreateScaledFont(string fontFamily, float size, FontStyle style = FontStyle.Regular)
        {
            var baseFont = new Font(fontFamily, size, style);
            return dpiService.ScaleFont(baseFont);
        }

        /// <summary>
        /// Helper method to scale a size value
        /// </summary>
        protected Size ScaleSize(Size size)
        {
            return dpiService.ScaleSize(size);
        }

        /// <summary>
        /// Helper method to scale an integer value
        /// </summary>
        protected int ScaleValue(int value)
        {
            return dpiService.ScaleValue(value);
        }

        /// <summary>
        /// Helper method to scale padding
        /// </summary>
        protected Padding ScalePadding(Padding padding)
        {
            return dpiService.ScalePadding(padding);
        }

        protected override void SetBoundsCore(int x, int y, int width, int height, BoundsSpecified specified)
        {
            // Apply DPI scaling to form bounds if high DPI
            if (dpiService != null && dpiService.IsHighDpi)
            {
                var scaledSize = dpiService.ScaleSize(new Size(width, height));
                base.SetBoundsCore(x, y, scaledSize.Width, scaledSize.Height, specified);
            }
            else
            {
                base.SetBoundsCore(x, y, width, height, specified);
            }
        }
    }
}