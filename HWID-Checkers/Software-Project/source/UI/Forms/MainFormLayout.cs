using System.Drawing;
using System.Windows.Forms;
using HWIDChecker.UI.Components;
using System.Runtime.InteropServices;
using System.Collections.Generic;
using System.Linq;

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
        public Button SectionedViewButton { get; private set; }
        public Label LoadingLabel { get; private set; }

        public MainFormLayout()
        {
            // No DPI service needed - Windows Forms handles it automatically
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
            
            // Use native .NET DPI handling
            form.Size = new Size(790, 820);
            form.StartPosition = FormStartPosition.CenterScreen;
            form.BackColor = ThemeColors.MainBackground;
            form.ForeColor = ThemeColors.PrimaryText;

            // Use native DPI scaling
            form.AutoScaleMode = AutoScaleMode.Dpi;

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
            // Create font - Windows Forms will handle scaling automatically
            var font = new Font("Consolas", 9.75f, FontStyle.Regular);

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
                Font = font
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
                AutoSize = true,
                MinimumSize = new Size(120, 35)
            };
            Buttons.ApplyStyle(RefreshButton);

            ExportButton = new Button
            {
                Text = "Export",
                AutoSize = true,
                MinimumSize = new Size(120, 35)
            };
            Buttons.ApplyStyle(ExportButton);

            CleanDevicesButton = new Button
            {
                Text = "Clean Devices",
                AutoSize = true,
                MinimumSize = new Size(100, 35)
            };
            Buttons.ApplyStyle(CleanDevicesButton);

            CleanLogsButton = new Button
            {
                Text = "Clean Logs",
                AutoSize = true,
                MinimumSize = new Size(100, 35)
            };
            Buttons.ApplyStyle(CleanLogsButton);

            CheckUpdatesButton = new Button
            {
                Text = "Check Updates",
                AutoSize = true,
                MinimumSize = new Size(110, 35)
            };
            Buttons.ApplyStyle(CheckUpdatesButton);

            SectionedViewButton = new Button
            {
                Text = "Sectioned View",
                AutoSize = true,
                MinimumSize = new Size(110, 35)
            };
            Buttons.ApplyStyle(SectionedViewButton);
        }

        private FlowLayoutPanel CreateButtonPanel(int formWidth)
        {
            var buttonPanel = new FlowLayoutPanel
            {
                Height = 60, // Base height - Windows Forms will scale automatically
                Dock = DockStyle.Bottom,
                BackColor = ThemeColors.ButtonPanelBackground,
                FlowDirection = FlowDirection.LeftToRight,
                WrapContents = true, // Allow wrapping at high DPI
                AutoSize = false,
                Width = formWidth
            };

            // Create a panel for centered buttons
            var centeredButtonPanel = new FlowLayoutPanel
            {
                AutoSize = true,
                FlowDirection = FlowDirection.LeftToRight,
                WrapContents = true, // Allow wrapping at high DPI
                BackColor = ThemeColors.ButtonPanelBackground
            };

            // Set consistent margins for all buttons - Windows Forms will scale automatically
            var buttonMargin = new Padding(5);
            RefreshButton.Margin = buttonMargin;
            ExportButton.Margin = buttonMargin;
            CleanDevicesButton.Margin = buttonMargin;
            CleanLogsButton.Margin = buttonMargin;
            CheckUpdatesButton.Margin = buttonMargin;
            SectionedViewButton.Margin = buttonMargin;

            centeredButtonPanel.Controls.AddRange(new Control[] { RefreshButton, ExportButton, CleanDevicesButton, CleanLogsButton, CheckUpdatesButton, SectionedViewButton });

            // Simple centering - let Windows Forms handle the scaling
            var panelMargin = new Padding(10);
            centeredButtonPanel.Margin = panelMargin;

            buttonPanel.Controls.Add(centeredButtonPanel);

            return buttonPanel;
        }

        public string GetAllContentForExport()
        {
            // Return the content from OutputTextBox for export compatibility
            return OutputTextBox.Text;
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