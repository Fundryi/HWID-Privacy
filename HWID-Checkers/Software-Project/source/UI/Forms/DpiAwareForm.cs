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
        /// Apply conservative DPI scaling to the form after all controls have been added
        /// This prevents forms from becoming too large on high DPI displays
        /// </summary>
        protected void ApplyDpiScaling()
        {
            // Don't apply aggressive scaling that can make forms unusable
            // Instead rely on individual control scaling and Windows' built-in DPI handling
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
        /// Helper method to scale a size value conservatively
        /// </summary>
        protected Size ScaleSize(Size size)
        {
            return dpiService.ScaleSizeConservative(size);
        }

        /// <summary>
        /// Helper method to scale an integer value conservatively
        /// </summary>
        protected int ScaleValue(int value)
        {
            return dpiService.ScaleValueConservative(value);
        }

        /// <summary>
        /// Helper method to scale padding conservatively
        /// </summary>
        protected Padding ScalePadding(Padding padding)
        {
            var scaledValue = dpiService.ScaleValueConservative(5); // Use a reasonable default
            return new Padding(scaledValue);
        }

        protected override void SetBoundsCore(int x, int y, int width, int height, BoundsSpecified specified)
        {
            // Apply conservative DPI scaling to form bounds to prevent oversized windows
            if (dpiService != null && dpiService.IsHighDpi)
            {
                var scaledSize = dpiService.ScaleSizeConservative(new Size(width, height));
                base.SetBoundsCore(x, y, scaledSize.Width, scaledSize.Height, specified);
            }
            else
            {
                base.SetBoundsCore(x, y, width, height, specified);
            }
        }
    }
}