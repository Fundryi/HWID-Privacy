using System;
using System.Drawing;
using System.Windows.Forms;
using HWIDChecker.UI.Forms;
using HWIDChecker.Hardware;

namespace HWIDChecker;

static class Program
{
    [STAThread]
    static void Main()
    {
        // CRITICAL: EnableVisualStyles must be called FIRST for proper DPI scaling
        Application.EnableVisualStyles();
        Application.SetCompatibleTextRenderingDefault(false);
        
        // Native .NET DPI handling - ApplicationHighDpiMode is set in .csproj
        Application.SetHighDpiMode(HighDpiMode.PerMonitorV2);
        
        ApplicationConfiguration.Initialize();
        
        try
        {
            using (var stream = typeof(Program).Assembly.GetManifestResourceStream("HWIDChecker.Resources.app.ico"))
            {
                if (stream != null)
                {
                    var icon = new Icon(stream);
                    // Use the modern SectionedViewForm as the main window
                    var mainForm = new SectionedViewForm(isMainWindow: true) { Icon = icon };
                    Application.Run(mainForm);
                }
                else
                {
                    // Use the modern SectionedViewForm as the main window
                    var mainForm = new SectionedViewForm(isMainWindow: true);
                    Application.Run(mainForm);
                }
            }
        }
        catch (Exception ex)
        {
            System.Diagnostics.Debug.WriteLine($"Failed to load application icon: {ex.Message}");
            // Fallback: Use the modern SectionedViewForm as the main window
            var mainForm = new SectionedViewForm(isMainWindow: true);
            Application.Run(mainForm);
        }
    }
}