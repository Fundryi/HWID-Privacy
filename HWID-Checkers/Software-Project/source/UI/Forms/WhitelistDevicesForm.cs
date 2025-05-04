using System;
using System.Windows.Forms;
using System.Drawing;
using System.Collections.Generic;
using HWIDChecker.Services;
using HWIDChecker.Services.Models;

namespace HWIDChecker.UI.Forms
{
    public class WhitelistDevicesForm : Form
    {
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
            this.Width = 800;
            this.Height = 600;
            this.StartPosition = FormStartPosition.CenterScreen;
            this.FormBorderStyle = FormBorderStyle.FixedDialog;
            this.MaximizeBox = false;
            this.MinimizeBox = false;

            devicesListBox = new CheckedListBox
            {
                Dock = DockStyle.Fill,
                CheckOnClick = true,
                Font = new Font("Consolas", 9.75f, FontStyle.Regular)
            };

            resetWhitelistButton = new Button
            {
                Text = "Reset Whitelist",
                Dock = DockStyle.Bottom,
                Height = 30
            };
            resetWhitelistButton.Click += ResetWhitelist_Click;

            confirmButton = new Button
            {
                Text = "Save Whitelist",
                Dock = DockStyle.Bottom,
                Height = 30
            };
            confirmButton.Click += Confirm_Click;

            cancelButton = new Button
            {
                Text = "Cancel",
                Dock = DockStyle.Bottom,
                Height = 30
            };
            cancelButton.Click += (s, e) => this.DialogResult = DialogResult.Cancel;

            var panel = new Panel
            {
                Dock = DockStyle.Fill,
                Padding = new Padding(10)
            };
            panel.Controls.Add(devicesListBox);

            this.Controls.Add(panel);
            this.Controls.Add(resetWhitelistButton);
            this.Controls.Add(confirmButton);
            this.Controls.Add(cancelButton);
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
    }
}