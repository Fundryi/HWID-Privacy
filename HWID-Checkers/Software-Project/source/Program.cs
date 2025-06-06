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
        try
        {
            using (var stream = typeof(Program).Assembly.GetManifestResourceStream("HWIDChecker.Resources.app.ico"))
            {
                if (stream != null)
                {
                    var icon = new Icon(stream);
                    ApplicationConfiguration.Initialize();
                    Application.EnableVisualStyles();
                    Application.SetCompatibleTextRenderingDefault(false);
                    
                    // Use the modern SectionedViewForm as the main window
                    var mainForm = new SectionedViewForm(isMainWindow: true) { Icon = icon };
                    Application.Run(mainForm);
                }
                else
                {
                    ApplicationConfiguration.Initialize();
                    Application.EnableVisualStyles();
                    Application.SetCompatibleTextRenderingDefault(false);
                    
                    // Use the modern SectionedViewForm as the main window
                    var mainForm = new SectionedViewForm(isMainWindow: true);
                    Application.Run(mainForm);
                }
            }
        }
        catch (Exception ex)
        {
            System.Diagnostics.Debug.WriteLine($"Failed to load application icon: {ex.Message}");
            ApplicationConfiguration.Initialize();
            Application.EnableVisualStyles();
            Application.SetCompatibleTextRenderingDefault(false);
            
            // Fallback: Use the modern SectionedViewForm as the main window
            var mainForm = new SectionedViewForm(isMainWindow: true);
            Application.Run(mainForm);
        }
    }
}