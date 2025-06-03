using System.Drawing;
using System.Windows.Forms;
using HWIDChecker.UI.Components;
using HWIDChecker.Services;
using System.Runtime.InteropServices;

namespace HWIDChecker.UI.Forms
{
    public class MainFormLayout
    {
        private readonly DpiScalingService dpiService;
        
        public TextBox OutputTextBox { get; private set; }
        public Button RefreshButton { get; private set; }
        public Button ExportButton { get; private set; }
        public Button CleanDevicesButton { get; private set; }
        public Button CleanLogsButton { get; private set; }
        public Button CheckUpdatesButton { get; private set; }
        public Label LoadingLabel { get; private set; }

        public MainFormLayout()
        {
            dpiService = DpiScalingService.Instance;
        }

        [DllImport("user32.dll")]
        private static extern int GetWindowLong(IntPtr hWnd, int nIndex);

        [DllImport("user32.dll")]
        private static extern int SetWindowLong(IntPtr hWnd, int nIndex, int dwNewLong);

        private const int GWL_STYLE = -16;
        private const int WS_HSCROLL = 0x00100000;
        private const int WS_VSCROLL = 0x00200000;

        public void InitializeLayout(Form form)
        {
            try
            {
                // Set icon first to ensure it's loaded before other UI elements
                using (var stream = GetType().Assembly.GetManifestResourceStream("HWIDChecker.Resources.app.ico"))
                {
                    if (stream != null)
                    {
                        form.Icon = new Icon(stream);
                    }
                }
            }
            catch (Exception ex)
            {
                System.Diagnostics.Debug.WriteLine($"Failed to load icon: {ex.Message}");
            }

            form.Text = "HWID Checker";
            
            // Apply DPI scaling to form size
            var baseSize = new Size(790, 820);
            form.Size = dpiService.ScaleSize(baseSize);
            
            form.StartPosition = FormStartPosition.CenterScreen;
            form.BackColor = ThemeColors.MainBackground;
            form.ForeColor = ThemeColors.PrimaryText;

            // Enable automatic scaling for the form
            form.AutoScaleMode = AutoScaleMode.Dpi;

            InitializeControls();
            var buttonPanel = CreateButtonPanel(form.Width);

            form.Controls.AddRange(new Control[] { OutputTextBox, buttonPanel, LoadingLabel });

            // Apply DPI scaling to all controls
            dpiService.ScaleControl(form);

            // Enable dark scrollbars for Windows 10 and later
            if (Environment.OSVersion.Version.Major >= 10)
            {
                SetWindowTheme(OutputTextBox.Handle, "DarkMode_Explorer", null);
                int style = GetWindowLong(OutputTextBox.Handle, GWL_STYLE);
                style = style | WS_VSCROLL;
                style = style & ~WS_HSCROLL;  // Remove horizontal scrollbar
                SetWindowLong(OutputTextBox.Handle, GWL_STYLE, style);
            }
        }

        [DllImport("uxtheme.dll", CharSet = CharSet.Unicode)]
        private static extern int SetWindowTheme(IntPtr hWnd, string pszSubAppName, string pszSubIdList);

        private void InitializeControls()
        {
            // Create base font and scale it
            var baseFont = new Font("Consolas", 9.75f, FontStyle.Regular);
            var scaledFont = dpiService.ScaleFont(baseFont);

            OutputTextBox = new TextBox
            {
                Multiline = true,
                ReadOnly = true,
                ScrollBars = ScrollBars.Vertical,
                Dock = DockStyle.Fill,
                WordWrap = true,
                BackColor = ThemeColors.TextBoxBackground,
                ForeColor = ThemeColors.TextBoxText,
                BorderStyle = BorderStyle.None,
                Margin = dpiService.ScalePadding(new Padding(10)),
                Padding = dpiService.ScalePadding(new Padding(5)),
                HideSelection = false,
                Font = scaledFont
            };

            LoadingLabel = new Label
            {
                Text = "Fetching HWID data...",
                AutoSize = true,
                ForeColor = ThemeColors.LoadingLabelText,
                BackColor = ThemeColors.LoadingLabelBackground,
                Visible = false,
                Padding = dpiService.ScalePadding(new Padding(5)),
                TextAlign = ContentAlignment.MiddleCenter
            };

            InitializeButtons();
        }

        private void InitializeButtons()
        {
            RefreshButton = new Button
            {
                Text = "Refresh",
                Height = dpiService.ScaleValue(20),
                AutoSize = true,
                MinimumSize = dpiService.ScaleSize(new Size(120, 35))
            };
            Buttons.ApplyStyle(RefreshButton);

            ExportButton = new Button
            {
                Text = "Export",
                Height = dpiService.ScaleValue(20),
                AutoSize = true,
                MinimumSize = dpiService.ScaleSize(new Size(120, 35))
            };
            Buttons.ApplyStyle(ExportButton);

            CleanDevicesButton = new Button
            {
                Text = "Clean Devices",
                Height = dpiService.ScaleValue(20),
                AutoSize = true,
                MinimumSize = dpiService.ScaleSize(new Size(100, 35))
            };
            Buttons.ApplyStyle(CleanDevicesButton);

            CleanLogsButton = new Button
            {
                Text = "Clean Logs",
                Height = dpiService.ScaleValue(20),
                AutoSize = true,
                MinimumSize = dpiService.ScaleSize(new Size(100, 35))
            };
            Buttons.ApplyStyle(CleanLogsButton);

            CheckUpdatesButton = new Button
            {
                Text = "Check Updates",
                Height = dpiService.ScaleValue(20),
                AutoSize = true,
                MinimumSize = dpiService.ScaleSize(new Size(110, 35))
            };
            Buttons.ApplyStyle(CheckUpdatesButton);
        }

        private FlowLayoutPanel CreateButtonPanel(int formWidth)
        {
            var buttonPanel = new FlowLayoutPanel
            {
                Height = dpiService.ScaleValue(60),
                Dock = DockStyle.Bottom,
                BackColor = ThemeColors.ButtonPanelBackground,
                FlowDirection = FlowDirection.LeftToRight,
                WrapContents = false,
                AutoSize = false,
                Width = formWidth
            };

            // Create a panel for centered buttons
            var centeredButtonPanel = new FlowLayoutPanel
            {
                AutoSize = true,
                FlowDirection = FlowDirection.LeftToRight,
                WrapContents = false,
                BackColor = ThemeColors.ButtonPanelBackground
            };

            // Set consistent margins for all buttons with DPI scaling
            var buttonMargin = dpiService.ScalePadding(new Padding(5, 5, 5, 5));
            RefreshButton.Margin = buttonMargin;
            ExportButton.Margin = buttonMargin;
            CleanDevicesButton.Margin = buttonMargin;
            CleanLogsButton.Margin = buttonMargin;
            CheckUpdatesButton.Margin = buttonMargin;

            centeredButtonPanel.Controls.AddRange(new Control[] { RefreshButton, ExportButton, CleanDevicesButton, CleanLogsButton, CheckUpdatesButton });

            // Calculate center position for buttons with DPI scaling
            int scaledSpacing = dpiService.ScaleValue(35);
            int totalCenteredWidth = RefreshButton.Width + ExportButton.Width + CleanDevicesButton.Width + CleanLogsButton.Width + CheckUpdatesButton.Width + scaledSpacing;
            int startX = (buttonPanel.Width - totalCenteredWidth) / 2;
            centeredButtonPanel.Margin = dpiService.ScalePadding(new Padding(Math.Max(0, startX), 10, 0, 10));

            buttonPanel.Controls.Add(centeredButtonPanel);

            return buttonPanel;
        }

        public void UpdateLoadingLabelPosition(Form form)
        {
            LoadingLabel.Location = new Point(
                (form.ClientSize.Width - LoadingLabel.Width) / 2,
                (form.ClientSize.Height - LoadingLabel.Height) / 2
            );
        }
    }
}