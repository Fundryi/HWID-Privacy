using System.Drawing;
using System.Windows.Forms;
using HWIDChecker.UI.Components;
using System.Runtime.InteropServices;

namespace HWIDChecker.UI.Forms
{
    public class MainFormLayout
    {
        public TextBox OutputTextBox { get; private set; }
        public Button RefreshButton { get; private set; }
        public Button ExportButton { get; private set; }
        public Button CleanDevicesButton { get; private set; }
        public Button CleanLogsButton { get; private set; }
        public Button CheckUpdatesButton { get; private set; }
        public Label LoadingLabel { get; private set; }

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
            form.Size = new Size(790, 820);
            form.StartPosition = FormStartPosition.CenterScreen;
            form.BackColor = ThemeColors.MainBackground;
            form.ForeColor = ThemeColors.PrimaryText;

            InitializeControls();
            var buttonPanel = CreateButtonPanel(form.Width);

            form.Controls.AddRange(new Control[] { OutputTextBox, buttonPanel, LoadingLabel });

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
                Margin = new Padding(10),
                Padding = new Padding(5),
                HideSelection = false,
                Font = new Font("Consolas", 9.75f, FontStyle.Regular)
            };

            LoadingLabel = new Label
            {
                Text = "Fetching HWID data...",
                AutoSize = true,
                ForeColor = ThemeColors.LoadingLabelText,
                BackColor = ThemeColors.LoadingLabelBackground,
                Visible = false,
                Padding = new Padding(5),
                TextAlign = ContentAlignment.MiddleCenter
            };

            InitializeButtons();
        }

        private void InitializeButtons()
        {
            RefreshButton = new Button
            {
                Text = "Refresh",
                Height = 20,
                AutoSize = true,
                MinimumSize = new Size(120, 35)
            };
            Buttons.ApplyStyle(RefreshButton);

            ExportButton = new Button
            {
                Text = "Export",
                Height = 20,
                AutoSize = true,
                MinimumSize = new Size(120, 35)
            };
            Buttons.ApplyStyle(ExportButton);

            CleanDevicesButton = new Button
            {
                Text = "Clean Devices",
                Height = 20,
                AutoSize = true,
                MinimumSize = new Size(100, 35)
            };
            Buttons.ApplyStyle(CleanDevicesButton);

            CleanLogsButton = new Button
            {
                Text = "Clean Logs",
                Height = 20,
                AutoSize = true,
                MinimumSize = new Size(100, 35)
            };
            Buttons.ApplyStyle(CleanLogsButton);

            CheckUpdatesButton = new Button
            {
                Text = "Check Updates",
                Height = 20,
                AutoSize = true,
                MinimumSize = new Size(110, 35)
            };
            Buttons.ApplyStyle(CheckUpdatesButton);
        }

        private FlowLayoutPanel CreateButtonPanel(int formWidth)
        {
            var buttonPanel = new FlowLayoutPanel
            {
                Height = 60,
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

            // Set consistent margins for all buttons
            RefreshButton.Margin = new Padding(5, 5, 5, 5);
            ExportButton.Margin = new Padding(5, 5, 5, 5);
            CleanDevicesButton.Margin = new Padding(5, 5, 5, 5);
            CleanLogsButton.Margin = new Padding(5, 5, 5, 5);
            CheckUpdatesButton.Margin = new Padding(5, 5, 5, 5);

            centeredButtonPanel.Controls.AddRange(new Control[] { RefreshButton, ExportButton, CleanDevicesButton, CleanLogsButton, CheckUpdatesButton });

            // Calculate center position for buttons
            int totalCenteredWidth = RefreshButton.Width + ExportButton.Width + CleanDevicesButton.Width + CleanLogsButton.Width + CheckUpdatesButton.Width + 35;
            int startX = (buttonPanel.Width - totalCenteredWidth) / 2;
            centeredButtonPanel.Margin = new Padding(Math.Max(0, startX), 10, 0, 10);

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