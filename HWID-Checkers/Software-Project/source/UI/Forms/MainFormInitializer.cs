using System;
using System.Windows.Forms;
using HWIDChecker.Hardware;
using HWIDChecker.Services;
using HWIDChecker.UI.Components;

namespace HWIDChecker.UI.Forms
{
    public class MainFormInitializer
    {
        private readonly MainForm mainForm;
        private readonly MainFormLayout layout;
        private readonly MainFormLoader loader;
        private readonly MainFormEventHandlers eventHandlers;
        private readonly HardwareInfoManager hardwareInfoManager;

        public MainFormInitializer(MainForm mainForm)
        {
            this.mainForm = mainForm;
            this.layout = new MainFormLayout();

            this.hardwareInfoManager = new HardwareInfoManager();
            var fileExportService = new FileExportService(Application.StartupPath);

            this.loader = new MainFormLoader(mainForm, layout, hardwareInfoManager);
            this.eventHandlers = new MainFormEventHandlers(mainForm, layout, fileExportService, hardwareInfoManager);
        }

        public void Initialize()
        {
            InitializeLayout();
            InitializeEventHandlers();
            InitializeLoadHandler();
        }

        private void InitializeLayout()
        {
            layout.InitializeLayout(mainForm);
        }

        private void InitializeEventHandlers()
        {
            eventHandlers.InitializeEventHandlers(loader.LoadHardwareInfo);
        }

        private void InitializeLoadHandler()
        {
            mainForm.Load += MainForm_Load;
        }

        private async void MainForm_Load(object sender, EventArgs e)
        {
            layout.UpdateLoadingLabelPosition(mainForm);
            await loader.LoadHardwareInfo();
        }

        public MainFormLayout Layout => layout;
    }
}