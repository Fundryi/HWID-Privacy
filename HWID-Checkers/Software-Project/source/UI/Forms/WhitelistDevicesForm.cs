using System;
using System.Windows.Forms;
using System.Drawing;
using System.Collections.Generic;
using HWIDChecker.Services;
using HWIDChecker.Services.Models;
using HWIDChecker.UI.Components;

namespace HWIDChecker.UI.Forms
{
    public class WhitelistDevicesForm : Form
    {
        private const int DefaultWidth = 920;
        private const int DefaultHeight = 640;
        private const int MinimumWidth = 720;
        private const int MinimumHeight = 520;
        private CheckedListBox devicesListBox;
        private Button confirmButton;
        private Button cancelButton;
        private Button resetWhitelistButton;
        private DeviceWhitelistService whitelistService;
        private List<DeviceDetail> devices;

        public WhitelistDevicesForm(List<DeviceDetail> ghostDevices)
        {
            this.devices = ghostDevices;
            this.whitelistService = new DeviceWhitelistService();
            InitializeComponents();
            LoadDevices();
        }

        private void InitializeComponents()
        {
            this.Text = "Manage Device Whitelist";
            this.StartPosition = FormStartPosition.CenterParent;
            this.FormBorderStyle = FormBorderStyle.Sizable;
            this.MaximizeBox = true;
            this.MinimizeBox = false;
            this.AutoScaleMode = AutoScaleMode.Dpi;
            this.ClientSize = new Size(DefaultWidth, DefaultHeight);
            this.MinimumSize = new Size(MinimumWidth, MinimumHeight);
            this.DpiChanged += WhitelistDevicesForm_DpiChanged;

            var headerLabel = new Label
            {
                Text = "Select ghost devices to keep in the whitelist:",
                AutoSize = true,
                Font = new Font("Segoe UI", 10f, FontStyle.Regular),
                ForeColor = SystemColors.ControlText,
                Margin = new Padding(12, 12, 12, 4),
                Anchor = AnchorStyles.Left
            };

            devicesListBox = new CheckedListBox
            {
                Dock = DockStyle.Fill,
                CheckOnClick = true,
                Font = new Font("Consolas", 9.75f, FontStyle.Regular),
                IntegralHeight = false
            };

            resetWhitelistButton = CreateActionButton("Reset Whitelist");
            resetWhitelistButton.Click += ResetWhitelist_Click;

            confirmButton = CreateActionButton("Save Whitelist");
            confirmButton.Click += Confirm_Click;

            cancelButton = CreateActionButton("Cancel");
            cancelButton.DialogResult = DialogResult.Cancel;

            var listPanel = new Panel
            {
                Dock = DockStyle.Fill,
                Padding = new Padding(10)
            };
            listPanel.Controls.Add(devicesListBox);

            var actionButtonPanel = new FlowLayoutPanel
            {
                Dock = DockStyle.Fill,
                FlowDirection = FlowDirection.RightToLeft,
                WrapContents = false,
                Padding = new Padding(10, 10, 10, 10),
                BackColor = SystemColors.Control
            };
            actionButtonPanel.Controls.Add(cancelButton);
            actionButtonPanel.Controls.Add(confirmButton);
            actionButtonPanel.Controls.Add(resetWhitelistButton);

            var mainLayout = new TableLayoutPanel
            {
                Dock = DockStyle.Fill,
                ColumnCount = 1,
                RowCount = 3
            };
            mainLayout.ColumnStyles.Add(new ColumnStyle(SizeType.Percent, 100F));
            mainLayout.RowStyles.Add(new RowStyle(SizeType.AutoSize));
            mainLayout.RowStyles.Add(new RowStyle(SizeType.Percent, 100F));
            mainLayout.RowStyles.Add(new RowStyle(SizeType.Absolute, 56F));
            mainLayout.Controls.Add(headerLabel, 0, 0);
            mainLayout.Controls.Add(listPanel, 0, 1);
            mainLayout.Controls.Add(actionButtonPanel, 0, 2);

            this.Controls.Add(mainLayout);
            this.AcceptButton = confirmButton;
            this.CancelButton = cancelButton;
        }

        private Button CreateActionButton(string text)
        {
            var button = new Button
            {
                Text = text,
                AutoSize = true,
                AutoSizeMode = AutoSizeMode.GrowAndShrink,
                MinimumSize = new Size(130, 34),
                Padding = new Padding(12, 4, 12, 4),
                Margin = new Padding(0, 0, 8, 0),
                UseVisualStyleBackColor = true
            };

            Buttons.ApplyStyle(button, Buttons.ButtonVariant.Secondary);
            return button;
        }

        private void LoadDevices()
        {
            devicesListBox.Items.Clear();
            var whitelistedDevices = whitelistService.LoadWhitelistedDevices();

            foreach (var device in devices)
            {
                var displayText = $"{device.Description} ({device.Class})";
                var index = devicesListBox.Items.Add(displayText);
                
                // Check the item if it's already whitelisted
                if (whitelistedDevices.Exists(d => d.HardwareId == device.HardwareId))
                {
                    devicesListBox.SetItemChecked(index, true);
                }
            }
        }

        private void Confirm_Click(object sender, EventArgs e)
        {
            var whitelistedDevices = new List<DeviceDetail>();

            for (int i = 0; i < devicesListBox.Items.Count; i++)
            {
                if (devicesListBox.GetItemChecked(i))
                {
                    whitelistedDevices.Add(devices[i]);
                }
            }

            whitelistService.SaveWhitelistedDevices(whitelistedDevices);
            this.DialogResult = DialogResult.OK;
        }

        private void ResetWhitelist_Click(object sender, EventArgs e)
        {
            if (MessageBox.Show(
                "Are you sure you want to reset the whitelist? This will remove all whitelisted devices.",
                "Confirm Reset",
                MessageBoxButtons.YesNo,
                MessageBoxIcon.Warning) == DialogResult.Yes)
            {
                whitelistService.ResetWhitelist();
                for (int i = 0; i < devicesListBox.Items.Count; i++)
                {
                    devicesListBox.SetItemChecked(i, false);
                }
            }
        }

        private void WhitelistDevicesForm_DpiChanged(object sender, DpiChangedEventArgs e)
        {
            this.PerformAutoScale();
            this.Invalidate();
        }
    }
}
