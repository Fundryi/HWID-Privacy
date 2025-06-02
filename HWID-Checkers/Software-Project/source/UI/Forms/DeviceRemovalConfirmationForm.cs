using System;
using System.Drawing;
using System.Windows.Forms;

namespace HWIDChecker.UI.Forms
{
    public class DeviceRemovalConfirmationForm : Form
    {
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
            this.Size = new Size(400, 160);
            this.StartPosition = FormStartPosition.CenterParent;
            this.FormBorderStyle = FormBorderStyle.FixedDialog;
            this.MaximizeBox = false;
            this.MinimizeBox = false;
            this.BackColor = Color.FromArgb(45, 45, 48);
            this.ForeColor = Color.White;

            // Message Label - centered
            messageLabel = new Label
            {
                Text = $"Remove {deviceCount} ghost devices?",
                Location = new Point(0, 15),
                Size = new Size(400, 20),
                Font = new Font("Segoe UI", 10f, FontStyle.Regular),
                ForeColor = Color.White,
                BackColor = Color.Transparent,
                TextAlign = ContentAlignment.MiddleCenter
            };

            // Warning Label - centered
            warningLabel = new Label
            {
                Text = "Warning: This action cannot be undone",
                Location = new Point(0, 40),
                Size = new Size(400, 18),
                Font = new Font("Segoe UI", 8.5f, FontStyle.Italic),
                ForeColor = Color.Orange,
                BackColor = Color.Transparent,
                TextAlign = ContentAlignment.MiddleCenter
            };

            // Calculate button positions for perfect centering
            int buttonY = 75;
            int totalButtonWidth = 110 + 60 + 60 + 20; // buttons + spacing
            int startX = (400 - totalButtonWidth) / 2;

            // Yes (Autoclose) Button - Default/Highlighted
            yesAutoCloseButton = new Button
            {
                Text = "Yes (Autoclose)",
                Location = new Point(startX, buttonY),
                Size = new Size(110, 30),
                BackColor = Color.FromArgb(0, 122, 204),
                ForeColor = Color.White,
                FlatStyle = FlatStyle.Flat,
                Font = new Font("Segoe UI", 8.5f, FontStyle.Bold),
                TabIndex = 0
            };
            yesAutoCloseButton.FlatAppearance.BorderColor = Color.FromArgb(0, 150, 255);
            yesAutoCloseButton.FlatAppearance.BorderSize = 2;
            yesAutoCloseButton.Click += (s, e) => {
                Result = ConfirmationResult.YesAutoClose;
                this.DialogResult = DialogResult.OK;
                this.Close();
            };

            // Yes Button
            yesButton = new Button
            {
                Text = "Yes",
                Location = new Point(startX + 120, buttonY),
                Size = new Size(60, 30),
                BackColor = Color.FromArgb(60, 60, 63),
                ForeColor = Color.White,
                FlatStyle = FlatStyle.Flat,
                Font = new Font("Segoe UI", 8.5f, FontStyle.Regular),
                TabIndex = 1
            };
            yesButton.FlatAppearance.BorderColor = Color.FromArgb(80, 80, 83);
            yesButton.Click += (s, e) => {
                Result = ConfirmationResult.Yes;
                this.DialogResult = DialogResult.OK;
                this.Close();
            };

            // No Button
            noButton = new Button
            {
                Text = "No",
                Location = new Point(startX + 190, buttonY),
                Size = new Size(60, 30),
                BackColor = Color.FromArgb(60, 60, 63),
                ForeColor = Color.White,
                FlatStyle = FlatStyle.Flat,
                Font = new Font("Segoe UI", 8.5f, FontStyle.Regular),
                TabIndex = 2
            };
            noButton.FlatAppearance.BorderColor = Color.FromArgb(80, 80, 83);
            noButton.Click += (s, e) => {
                Result = ConfirmationResult.No;
                this.DialogResult = DialogResult.Cancel;
                this.Close();
            };

            // Add hover effects
            AddHoverEffects(yesAutoCloseButton, Color.FromArgb(0, 140, 230), Color.FromArgb(0, 122, 204));
            AddHoverEffects(yesButton, Color.FromArgb(80, 80, 83), Color.FromArgb(60, 60, 63));
            AddHoverEffects(noButton, Color.FromArgb(80, 80, 83), Color.FromArgb(60, 60, 63));

            // Add all controls
            this.Controls.AddRange(new Control[] { messageLabel, warningLabel, yesAutoCloseButton, yesButton, noButton });

            // Set default button (highlighted)
            this.AcceptButton = yesAutoCloseButton;
            this.CancelButton = noButton;

            // Focus on the default button
            yesAutoCloseButton.Focus();
        }

        private void AddHoverEffects(Button button, Color hoverColor, Color normalColor)
        {
            button.MouseEnter += (s, e) => button.BackColor = hoverColor;
            button.MouseLeave += (s, e) => button.BackColor = normalColor;
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
