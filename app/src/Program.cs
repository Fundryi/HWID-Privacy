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
        // .NET 8 pattern: ApplicationConfiguration.Initialize() FIRST
        ApplicationConfiguration.Initialize();
        
        // Then set DPI mode before any visual initialization
        Application.SetHighDpiMode(HighDpiMode.PerMonitorV2);
        
        // Then enable visual styles
        Application.EnableVisualStyles();
        Application.SetCompatibleTextRenderingDefault(false);
        
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