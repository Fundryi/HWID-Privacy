using System;
using System.Drawing;
using System.Windows.Forms;

namespace HWIDChecker.UI.Forms
{
    public class DeviceRemovalConfirmationForm : Form
    {
        private const int DialogWidth = 520;
        private const int DialogHeight = 210;
        private const int MinimumDialogWidth = 460;
        private const int MinimumDialogHeight = 190;
        private Button yesAutoCloseButton;
        private Button yesButton;
        private Button noButton;
        private Label messageLabel;
        private Label warningLabel;

        public enum ConfirmationResult
        {
            YesAutoClose,
            Yes,
            No
        }

        public ConfirmationResult Result { get; private set; } = ConfirmationResult.No;

        public DeviceRemovalConfirmationForm(int deviceCount)
        {
            InitializeComponents(deviceCount);
            Result = ConfirmationResult.No;
        }

        private void InitializeComponents(int deviceCount)
        {
            this.Text = "Confirm Device Removal";
            this.StartPosition = FormStartPosition.CenterParent;
            this.FormBorderStyle = FormBorderStyle.FixedDialog;
            this.MaximizeBox = false;
            this.MinimizeBox = false;
            this.AutoScaleMode = AutoScaleMode.Dpi;
            this.ClientSize = new Size(DialogWidth, DialogHeight);
            this.MinimumSize = new Size(MinimumDialogWidth, MinimumDialogHeight);
            this.BackColor = Color.FromArgb(45, 45, 48);
            this.ForeColor = Color.White;
            this.DpiChanged += DeviceRemovalConfirmationForm_DpiChanged;

            // Message label
            messageLabel = new Label
            {
                Text = $"Remove {deviceCount} ghost devices?",
                AutoSize = true,
                Font = new Font("Segoe UI", 10f, FontStyle.Regular),
                ForeColor = Color.White,
                BackColor = Color.Transparent,
                TextAlign = ContentAlignment.MiddleCenter,
                Anchor = AnchorStyles.None,
                Margin = new Padding(0, 0, 0, 4)
            };

            // Warning label
            warningLabel = new Label
            {
                Text = "Warning: This action cannot be undone",
                AutoSize = true,
                Font = new Font("Segoe UI", 8.5f, FontStyle.Italic),
                ForeColor = Color.Orange,
                BackColor = Color.Transparent,
                TextAlign = ContentAlignment.MiddleCenter,
                Anchor = AnchorStyles.None,
                Margin = new Padding(0, 0, 0, 10)
            };

            // Action buttons
            yesAutoCloseButton = CreateActionButton("Yes (Autoclose)", isPrimary: true, minWidth: 150);
            yesAutoCloseButton.TabIndex = 0;
            yesAutoCloseButton.Click += (s, e) => {
                Result = ConfirmationResult.YesAutoClose;
                this.DialogResult = DialogResult.OK;
                this.Close();
            };

            yesButton = CreateActionButton("Yes", isPrimary: false, minWidth: 80);
            yesButton.TabIndex = 1;
            yesButton.Click += (s, e) => {
                Result = ConfirmationResult.Yes;
                this.DialogResult = DialogResult.OK;
                this.Close();
            };

            noButton = CreateActionButton("No", isPrimary: false, minWidth: 80);
            noButton.TabIndex = 2;
            noButton.Click += (s, e) => {
                Result = ConfirmationResult.No;
                this.DialogResult = DialogResult.Cancel;
                this.Close();
            };

            var buttonPanel = new FlowLayoutPanel
            {
                AutoSize = true,
                AutoSizeMode = AutoSizeMode.GrowAndShrink,
                FlowDirection = FlowDirection.LeftToRight,
                WrapContents = true,
                BackColor = Color.Transparent,
                Anchor = AnchorStyles.None,
                Margin = new Padding(0)
            };
            buttonPanel.Controls.Add(yesAutoCloseButton);
            buttonPanel.Controls.Add(yesButton);
            buttonPanel.Controls.Add(noButton);

            var mainLayout = new TableLayoutPanel
            {
                Dock = DockStyle.Fill,
                ColumnCount = 1,
                RowCount = 3,
                Padding = new Padding(16)
            };
            mainLayout.ColumnStyles.Add(new ColumnStyle(SizeType.Percent, 100F));
            mainLayout.RowStyles.Add(new RowStyle(SizeType.AutoSize));
            mainLayout.RowStyles.Add(new RowStyle(SizeType.AutoSize));
            mainLayout.RowStyles.Add(new RowStyle(SizeType.AutoSize));
            mainLayout.Controls.Add(messageLabel, 0, 0);
            mainLayout.Controls.Add(warningLabel, 0, 1);
            mainLayout.Controls.Add(buttonPanel, 0, 2);

            this.Controls.Add(mainLayout);
            this.Resize += DeviceRemovalConfirmationForm_Resize;
            UpdateLabelWrapWidths();

            // Set default button
            this.AcceptButton = yesAutoCloseButton;
            this.CancelButton = noButton;

            yesAutoCloseButton.Focus();
        }

        private Button CreateActionButton(string text, bool isPrimary, int minWidth)
        {
            var button = new Button
            {
                Text = text,
                AutoSize = true,
                AutoSizeMode = AutoSizeMode.GrowAndShrink,
                MinimumSize = new Size(minWidth, 34),
                BackColor = isPrimary ? Color.FromArgb(0, 122, 204) : Color.FromArgb(60, 60, 63),
                ForeColor = Color.White,
                FlatStyle = FlatStyle.Flat,
                Font = new Font("Segoe UI", 8.5f, isPrimary ? FontStyle.Bold : FontStyle.Regular),
                Margin = new Padding(0, 0, 8, 8),
                Padding = new Padding(12, 4, 12, 4)
            };

            button.FlatAppearance.BorderColor = isPrimary
                ? Color.FromArgb(0, 150, 255)
                : Color.FromArgb(80, 80, 83);
            button.FlatAppearance.BorderSize = isPrimary ? 2 : 1;

            AddHoverEffects(
                button,
                isPrimary ? Color.FromArgb(0, 140, 230) : Color.FromArgb(80, 80, 83),
                isPrimary ? Color.FromArgb(0, 122, 204) : Color.FromArgb(60, 60, 63));

            return button;
        }

        private void AddHoverEffects(Button button, Color hoverColor, Color normalColor)
        {
            button.MouseEnter += (s, e) => button.BackColor = hoverColor;
            button.MouseLeave += (s, e) => button.BackColor = normalColor;
        }

        private void DeviceRemovalConfirmationForm_DpiChanged(object sender, DpiChangedEventArgs e)
        {
            this.PerformAutoScale();
            UpdateLabelWrapWidths();
            this.Invalidate();
        }

        private void DeviceRemovalConfirmationForm_Resize(object sender, EventArgs e)
        {
            UpdateLabelWrapWidths();
        }

        private void UpdateLabelWrapWidths()
        {
            var maxWidth = Math.Max(220, this.ClientSize.Width - 40);
            messageLabel.MaximumSize = new Size(maxWidth, 0);
            warningLabel.MaximumSize = new Size(maxWidth, 0);
        }

        protected override bool ProcessDialogKey(Keys keyData)
        {
            // Handle Enter key to activate the focused button
            if (keyData == Keys.Enter)
            {
                if (yesAutoCloseButton.Focused)
                {
                    yesAutoCloseButton.PerformClick();
                    return true;
                }
                else if (yesButton.Focused)
                {
                    yesButton.PerformClick();
                    return true;
                }
                else if (noButton.Focused)
                {
                    noButton.PerformClick();
                    return true;
                }
            }
            // Handle Escape key to close as No
            else if (keyData == Keys.Escape)
            {
                noButton.PerformClick();
                return true;
            }

            return base.ProcessDialogKey(keyData);
        }

        public static ConfirmationResult ShowConfirmation(IWin32Window owner, int deviceCount)
        {
            using (var form = new DeviceRemovalConfirmationForm(deviceCount))
            {
                var dialogResult = form.ShowDialog(owner);
                return form.Result;
            }
        }
    }
}
