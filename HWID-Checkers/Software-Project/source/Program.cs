using System;
using System.Drawing;
using System.Windows.Forms;
using HWIDChecker.UI.Forms;

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
                    Application.Run(new MainForm { Icon = icon });
                }
                else
                {
                    ApplicationConfiguration.Initialize();
                    Application.Run(new MainForm());
                }
            }
        }
        catch (Exception ex)
        {
            System.Diagnostics.Debug.WriteLine($"Failed to load application icon: {ex.Message}");
            ApplicationConfiguration.Initialize();
            Application.Run(new MainForm());
        }
    }
}