using System.Windows.Forms;
using HWIDChecker.UI.Forms;

namespace HWIDChecker.UI.Forms
{
    public partial class MainForm : Form
    {
        private readonly MainFormInitializer initializer;

        public MainForm()
        {
            // Let Windows Forms handle DPI automatically
            initializer = new MainFormInitializer(this);
            initializer.Initialize();
        }
    }
}