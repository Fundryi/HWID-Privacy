using System;
using System.Collections.Generic;
using System.Drawing;
using System.Windows.Forms;
using System.IO;
using HWIDChecker.UI.Components;
using HWIDChecker.Services;

namespace HWIDChecker.UI.Forms
{
    public class CompareForm : Form
    {
        private readonly Components.CompareFormLayout layout; // Explicitly specify Components namespace
        private readonly DpiScalingService dpiService;

        private CompareForm(string leftContent)
        {
            // Initialize DPI scaling before any UI operations
            dpiService = DpiScalingService.Instance;
            
            // Set icon
            try
            {
                using (var stream = GetType().Assembly.GetManifestResourceStream("HWIDChecker.Resources.app.ico"))
                {
                    if (stream != null)
                    {
                        Icon = new Icon(stream);
                    }
                }
            }
            catch (Exception ex)
            {
                System.Diagnostics.Debug.WriteLine($"Failed to load icon: {ex.Message}");
            }

            // Enable automatic scaling for the form
            AutoScaleMode = AutoScaleMode.Dpi;

            layout = new Components.CompareFormLayout(); // Explicitly specify Components namespace
            layout.InitializeLayout(this);

            var mainContainer = layout.CreateMainContainer();

            var leftPanel = layout.CreateSidePanel("Current Configuration");
            leftPanel.Controls.Add(layout.LeftText, 0, 1);
            // Set text and apply font
            layout.SetText(layout.LeftText, leftContent);

            var rightPanel = layout.CreateSidePanel("Comparison Configuration");
            rightPanel.Controls.Add(layout.RightText, 0, 1);

            mainContainer.Controls.Add(leftPanel, 0, 0);
            mainContainer.Controls.Add(rightPanel, 1, 0);

            Controls.Add(mainContainer);

            // Apply DPI scaling to all controls
            dpiService.ScaleControl(this);

            // Handle DPI changes at runtime
            this.DpiChanged += CompareForm_DpiChanged;
        }

        private void CompareForm_DpiChanged(object sender, DpiChangedEventArgs e)
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

        public static CompareForm CreateCompareWithCurrent(string currentConfig, List<string> exportFiles)
        {
            if (exportFiles.Count == 0) return null;

            var form = new CompareForm(currentConfig);
            form.layout.SetText(form.layout.RightText, File.ReadAllText(exportFiles[0]));
            return form;
        }

        public static CompareForm CreateCompareExported(List<string> exportFiles)
        {
            if (exportFiles.Count < 2) return null;

            var form = new CompareForm(File.ReadAllText(exportFiles[0]));
            form.layout.SetText(form.layout.RightText, File.ReadAllText(exportFiles[1]));
            return form;
        }
    }
}