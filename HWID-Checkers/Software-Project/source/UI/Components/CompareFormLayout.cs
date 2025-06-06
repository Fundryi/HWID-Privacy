using System;
using System.Drawing;
using System.Windows.Forms;
using System.Runtime.InteropServices;
using HWIDChecker.Utils;

namespace HWIDChecker.UI.Components
{
    public class CompareFormLayout
    {
        public RichTextBox LeftText { get; private set; }
        public RichTextBox RightText { get; private set; }
        private bool isScrolling = false;
        private int formWidth;
        private int formHeight;

        public CompareFormLayout()
        {
            // No DPI service needed - Windows Forms handles it automatically
        }

        [DllImport("uxtheme.dll", CharSet = CharSet.Unicode)]
        private static extern int SetWindowTheme(IntPtr hWnd, string pszSubAppName, string pszSubIdList);

        [DllImport("user32.dll")]
        private static extern int GetWindowLong(IntPtr hWnd, int nIndex);

        [DllImport("user32.dll")]
        private static extern int SetWindowLong(IntPtr hWnd, int nIndex, int dwNewLong);
        private const int GWL_STYLE = -16;
        private const int WS_HSCROLL = 0x00100000;
        private const int WS_VSCROLL = 0x00200000;


        public void SetText(RichTextBox textBox, string content)
        {
            textBox.Clear();
            var font = DpiHelper.CreateFont("Cascadia Code", 10);
            textBox.Font = font;
            textBox.Text = content;
            textBox.Select(0, 0);
        }

        public void InitializeLayout(Form form)
        {
            try
            {
                System.Diagnostics.Debug.WriteLine("Starting CompareFormLayout initialization...");

                // Use base size - Windows Forms will handle DPI scaling automatically
                var baseSize = DpiHelper.GetBaseFormSize(790 * 2, 800);
                this.formWidth = baseSize.Width;
                this.formHeight = baseSize.Height;

                System.Diagnostics.Debug.WriteLine($"Form dimensions: {this.formWidth}x{this.formHeight}");

                // Set form properties before size to prevent layout system from overriding
                form.AutoSize = false;
                form.AutoScaleMode = AutoScaleMode.Font; // Use Font-based scaling
                form.AutoScaleDimensions = new SizeF(96F, 96F);
                form.Text = "Compare HWID Information";
                form.MinimizeBox = true;
                form.MaximizeBox = true;
                form.WindowState = FormWindowState.Normal;
                form.BackColor = Color.FromArgb(30, 30, 30);
                form.ForeColor = Color.White;
                form.FormBorderStyle = FormBorderStyle.Sizable;

                // Set size and position after other properties
                form.ClientSize = new Size(this.formWidth, this.formHeight);
                form.StartPosition = FormStartPosition.Manual;
                form.MinimumSize = new Size(this.formWidth, this.formHeight); // Set minimum size
                form.MaximumSize = Screen.PrimaryScreen.WorkingArea.Size; // Allow resizing up to full screen

                System.Diagnostics.Debug.WriteLine("Initializing text boxes...");
                InitializeTextBoxes();
                System.Diagnostics.Debug.WriteLine("Text boxes initialized");

                System.Diagnostics.Debug.WriteLine("CompareFormLayout initialization completed");
            }
            catch (Exception ex)
            {
                System.Diagnostics.Debug.WriteLine($"Error in CompareFormLayout initialization: {ex}");
                MessageBox.Show(
                    $"Error initializing comparison layout:\n\n{ex.Message}",
                    "Layout Error",
                    MessageBoxButtons.OK,
                    MessageBoxIcon.Error);
                throw;
            }
        }

        private void InitializeTextBoxes()
        {
            try
            {
                LeftText = CreateCompareTextBox();
                RightText = CreateCompareTextBox();

                LeftText.VScroll += (s, e) => SynchronizeScrolling(LeftText, RightText);
                RightText.VScroll += (s, e) => SynchronizeScrolling(RightText, LeftText);
            }
            catch (Exception ex)
            {
                System.Diagnostics.Debug.WriteLine($"Error initializing text boxes: {ex}");
                throw;
            }
        }

        private RichTextBox CreateCompareTextBox()
        {
            try
            {
                var font = DpiHelper.CreateFont("Cascadia Code", 10, FontStyle.Regular);

                var textBox = new RichTextBox
                {
                    Multiline = true,
                    ReadOnly = true,
                    Dock = DockStyle.Fill,
                    Font = font,
                    BackColor = Color.FromArgb(45, 45, 45),
                    ForeColor = Color.FromArgb(220, 220, 220),
                    BorderStyle = BorderStyle.None,
                    Margin = DpiHelper.CreateMargin(10),
                    Padding = DpiHelper.CreatePadding(5),
                    HideSelection = false,
                    WordWrap = true,
                    ScrollBars = RichTextBoxScrollBars.Vertical
                };

                // Enable dark scrollbars for Windows 10 and later
                if (Environment.OSVersion.Version.Major >= 10)
                {
                    SetWindowTheme(textBox.Handle, "DarkMode_Explorer", null);
                    int style = GetWindowLong(textBox.Handle, GWL_STYLE);
                    style = style | WS_VSCROLL;
                    style = style & ~WS_HSCROLL;  // Remove horizontal scrollbar
                    SetWindowLong(textBox.Handle, GWL_STYLE, style);
                }

                return textBox;
            }
            catch (Exception ex)
            {
                System.Diagnostics.Debug.WriteLine($"Error creating compare text box: {ex}");
                throw;
            }
        }

        public TableLayoutPanel CreateMainContainer()
        {
            try
            {
                System.Diagnostics.Debug.WriteLine("Creating main container...");
                var mainContainer = new TableLayoutPanel
                {
                    Dock = DockStyle.Fill,
                    ColumnCount = 2,
                    RowCount = 1,
                    Padding = DpiHelper.CreatePadding(2),
                    Margin = DpiHelper.CreateMargin(0),
                    BackColor = Color.FromArgb(35, 35, 35),
                    AutoSize = false,
                    AutoSizeMode = AutoSizeMode.GrowAndShrink
                };
                mainContainer.ColumnStyles.Add(new ColumnStyle(SizeType.Percent, 50F));
                mainContainer.ColumnStyles.Add(new ColumnStyle(SizeType.Percent, 50F));
                System.Diagnostics.Debug.WriteLine("Main container created successfully");
                return mainContainer;
            }
            catch (Exception ex)
            {
                System.Diagnostics.Debug.WriteLine($"Error creating main container: {ex}");
                throw;
            }
        }

        public TableLayoutPanel CreateSidePanel(string labelText)
        {
            try
            {
                System.Diagnostics.Debug.WriteLine($"Creating side panel with label: {labelText}");
                var panel = new TableLayoutPanel
                {
                    Dock = DockStyle.Fill,
                    RowCount = 2,
                    ColumnCount = 1,
                    Padding = DpiHelper.CreatePadding(1),
                    Margin = DpiHelper.CreateMargin(1),
                    BackColor = Color.FromArgb(40, 40, 40),
                    AutoSize = false,
                    AutoSizeMode = AutoSizeMode.GrowAndShrink
                };
                
                var labelHeight = 20; // Base height - Windows Forms will scale automatically
                panel.RowStyles.Add(new RowStyle(SizeType.Absolute, labelHeight));
                panel.RowStyles.Add(new RowStyle(SizeType.Percent, 100F));

                var labelFont = DpiHelper.CreateFont("Segoe UI", 9f, FontStyle.Bold);

                var label = new Label
                {
                    Text = labelText,
                    Dock = DockStyle.Fill,
                    TextAlign = ContentAlignment.MiddleLeft,
                    Font = labelFont,
                    BackColor = Color.FromArgb(40, 40, 40),
                    ForeColor = Color.FromArgb(220, 220, 220),
                    AutoSize = false
                };

                panel.Controls.Add(label, 0, 0);
                System.Diagnostics.Debug.WriteLine($"Side panel created successfully: {labelText}");
                return panel;
            }
            catch (Exception ex)
            {
                System.Diagnostics.Debug.WriteLine($"Error creating side panel: {ex}");
                throw;
            }
        }

        private void SynchronizeScrolling(RichTextBox source, RichTextBox target)
        {
            if (isScrolling) return;

            try
            {
                isScrolling = true;

                int firstVisibleLine = source.GetLineFromCharIndex(source.GetCharIndexFromPosition(new Point(0, 0)));
                int targetPosition = target.GetFirstCharIndexFromLine(firstVisibleLine);

                if (targetPosition >= 0)
                {
                    target.SelectionStart = targetPosition;
                    target.ScrollToCaret();
                    target.SelectionLength = 0;
                }
            }
            catch (Exception ex)
            {
                System.Diagnostics.Debug.WriteLine($"Error synchronizing scrolling: {ex}");
            }
            finally
            {
                isScrolling = false;
            }
        }
    }
}