using System;
using System.Collections.Generic;
using System.Drawing;
using System.Linq;
using System.Windows.Forms;
using HWIDChecker.Hardware;
using HWIDChecker.Services;
using HWIDChecker.UI.Components;
using HWIDChecker.Utils;
using static HWIDChecker.Services.UpdateResult;

namespace HWIDChecker.UI.Forms
{
    public partial class SectionedViewForm : Form
    {
        private readonly HardwareInfoManager hardwareInfoManager;
        private string currentHardwareData;
        private Panel sidebarPanel;
        private Panel contentPanel;
        private TextBox currentContentTextBox;
        private Button refreshButton;
        private Button exportButton;
        private Button cleanDevicesButton;
        private Button cleanLogsButton;
        private Button checkUpdatesButton;
        private Label loadingLabel;
        private List<Button> sectionButtons;
        private List<(string title, string content)> sections;
        private bool isMainWindow;

        public SectionedViewForm(HardwareInfoManager hardwareInfoManager = null, string existingData = "", bool isMainWindow = false)
        {
            this.hardwareInfoManager = hardwareInfoManager ?? new HardwareInfoManager();
            this.currentHardwareData = existingData;
            this.sectionButtons = new List<Button>();
            this.sections = new List<(string, string)>();
            this.isMainWindow = isMainWindow;
            
            InitializeForm();
            
            if (isMainWindow && string.IsNullOrEmpty(existingData))
            {
                LoadHardwareDataAsync();
            }
        }

        private void InitializeForm()
        {
            // Set form properties BEFORE setting size to prevent scaling issues
            Text = isMainWindow ? "HWID Checker" : "HWID Checker - Sectioned View";
            BackColor = Color.FromArgb(32, 32, 32); // Darker background
            ForeColor = Color.White;
            FormBorderStyle = FormBorderStyle.FixedSingle; // Disable resizing
            StartPosition = isMainWindow ? FormStartPosition.CenterScreen : FormStartPosition.CenterParent;
            
            // Use standard Windows Forms DPI handling
            StandardDpiScaling.ConfigureForm(this);
            StandardDpiScaling.SetLogicalSize(this, 920, 710);

            CreateModernLayout();
            
            // Parse data into sections
            if (!string.IsNullOrEmpty(currentHardwareData))
            {
                ParseDataIntoSections(currentHardwareData);
                CreateSidebarButtons();
                if (sections.Count > 0)
                {
                    ShowSection(0); // Show first section by default
                }
            }
            else if (!isMainWindow)
            {
                CreateTestSection();
            }
        }
        
        // Removed SetDpiAwareSize() - now handled by DpiManager
        
        protected override void WndProc(ref Message m)
        {
            // Use standard Windows DPI change handling
            StandardDpiScaling.HandleDpiChange(this, ref m);
            base.WndProc(ref m);
        }

        private void CreateModernLayout()
        {
            // Create elegant sidebar (left side navigation)
            sidebarPanel = new Panel
            {
                Location = StandardDpiScaling.CreateLogicalPoint(0, 0),
                Size = StandardDpiScaling.CreateLogicalSize(280, ClientSize.Height - 60),
                BackColor = Color.FromArgb(40, 40, 40), // Slightly lighter than background
                BorderStyle = BorderStyle.None,
                Anchor = AnchorStyles.Top | AnchorStyles.Bottom | AnchorStyles.Left
            };

            // Add subtle border to sidebar
            var sidebarBorder = new Panel
            {
                Location = StandardDpiScaling.CreateLogicalPoint(279, 0),
                Size = StandardDpiScaling.CreateLogicalSize(1, ClientSize.Height - 60),
                BackColor = Color.FromArgb(60, 60, 60),
                Anchor = AnchorStyles.Top | AnchorStyles.Bottom | AnchorStyles.Left
            };

            // Create main content area
            contentPanel = new Panel
            {
                Location = StandardDpiScaling.CreateLogicalPoint(280, 0),
                Size = StandardDpiScaling.CreateLogicalSize(ClientSize.Width - 280, ClientSize.Height - 60),
                BackColor = Color.FromArgb(25, 25, 25), // Even darker for content
                BorderStyle = BorderStyle.None,
                Anchor = AnchorStyles.Top | AnchorStyles.Bottom | AnchorStyles.Left | AnchorStyles.Right,
                Padding = StandardDpiScaling.CreateLogicalPadding(20)
            };

            // Create content textbox with modern styling
            currentContentTextBox = new TextBox
            {
                Location = StandardDpiScaling.CreateLogicalPoint(20, 20),
                Size = StandardDpiScaling.CreateLogicalSize(contentPanel.Width - 40, contentPanel.Height - 40),
                Multiline = true,
                ReadOnly = true,
                ScrollBars = ScrollBars.Vertical,
                BackColor = Color.FromArgb(35, 35, 35),
                ForeColor = Color.FromArgb(220, 220, 220),
                Font = StandardDpiScaling.CreateLogicalFont("Consolas", 10f),
                BorderStyle = BorderStyle.None,
                Anchor = AnchorStyles.Top | AnchorStyles.Bottom | AnchorStyles.Left | AnchorStyles.Right
            };

            contentPanel.Controls.Add(currentContentTextBox);

            // Create modern button panel at bottom
            var buttonPanel = new Panel
            {
                Location = StandardDpiScaling.CreateLogicalPoint(0, ClientSize.Height - 60),
                Size = StandardDpiScaling.CreateLogicalSize(ClientSize.Width, 60),
                BackColor = Color.FromArgb(45, 45, 45),
                Anchor = AnchorStyles.Bottom | AnchorStyles.Left | AnchorStyles.Right
            };

            // Create loading label
            loadingLabel = new Label
            {
                Text = "Loading hardware information...",
                AutoSize = true,
                ForeColor = Color.FromArgb(220, 220, 220),
                BackColor = Color.Transparent,
                Font = StandardDpiScaling.CreateLogicalFont("Segoe UI", 10f),
                Visible = false
            };
            
            // Position loading label in center
            loadingLabel.Location = StandardDpiScaling.CreateLogicalPoint(
                (ClientSize.Width - loadingLabel.Width) / 2,
                (ClientSize.Height - loadingLabel.Height) / 2
            );

            // Create modern buttons based on main window status
            if (isMainWindow)
            {
                // Main window buttons with better spacing and fixed icons
                refreshButton = CreateModernButton("↻ Refresh", StandardDpiScaling.CreateLogicalPoint(20, 15));
                exportButton = CreateModernButton("💾 Export", StandardDpiScaling.CreateLogicalPoint(150, 15));
                cleanDevicesButton = CreateModernButton("🧹 Clean Devices", StandardDpiScaling.CreateLogicalPoint(280, 15));
                cleanLogsButton = CreateModernButton("📝 Clean Logs", StandardDpiScaling.CreateLogicalPoint(430, 15));
                checkUpdatesButton = CreateModernButton("⟳ Updates", StandardDpiScaling.CreateLogicalPoint(580, 15));

                // Add event handlers
                refreshButton.Click += RefreshButton_Click;
                exportButton.Click += ExportButton_Click;
                cleanDevicesButton.Click += CleanDevicesButton_Click;
                cleanLogsButton.Click += CleanLogsButton_Click;
                checkUpdatesButton.Click += CheckUpdatesButton_Click;

                buttonPanel.Controls.AddRange(new Control[] {
                    refreshButton, exportButton, cleanDevicesButton, cleanLogsButton, checkUpdatesButton
                });
            }
            else
            {
                // Sectioned view buttons (legacy mode)
                refreshButton = CreateModernButton("🔄 Refresh", new Point(20, 15));
                exportButton = CreateModernButton("💾 Export", new Point(140, 15));
                var closeButton = CreateModernButton("✖ Close", new Point(260, 15));

                // Add event handlers
                refreshButton.Click += RefreshButton_Click;
                exportButton.Click += ExportButton_Click;
                closeButton.Click += (s, e) => Close();

                buttonPanel.Controls.AddRange(new Control[] { refreshButton, exportButton, closeButton });
            }

            // Add all panels to form
            Controls.AddRange(new Control[] { sidebarPanel, sidebarBorder, contentPanel, buttonPanel, loadingLabel });
        }

        private Button CreateModernButton(string text, Point location)
        {
            return new Button
            {
                Location = location,
                Size = new Size(110, 30),
                Text = text,
                BackColor = Color.FromArgb(0, 120, 215), // Modern blue
                ForeColor = Color.White,
                FlatStyle = FlatStyle.Flat,
                Font = MonitorDpiPresets.CreatePresetFont(this, "Segoe UI", 9f),
                Cursor = Cursors.Hand
            };
        }

        private void ParseDataIntoSections(string allData)
        {
            sections.Clear();
            
            // Split by the actual section pattern using regex to preserve original formatting
            var sectionPattern = @"={20,}[\r\n]+\s*([^=\r\n]+?)\s*[\r\n]+={20,}";
            var matches = System.Text.RegularExpressions.Regex.Matches(allData, sectionPattern);
            
            if (matches.Count == 0)
            {
                // Fallback if pattern doesn't match
                sections.Add(("All Hardware Info", allData));
                return;
            }
            
            for (int i = 0; i < matches.Count; i++)
            {
                var match = matches[i];
                var title = match.Groups[1].Value.Trim();
                
                // Get the start of content (after this section's closing separator)
                var contentStart = match.Index + match.Length;
                
                // Get the end of content (before next section's opening separator, or end of data)
                var contentEnd = (i + 1 < matches.Count) ? matches[i + 1].Index : allData.Length;
                
                // Extract content exactly as it appears
                var content = allData.Substring(contentStart, contentEnd - contentStart).Trim();
                
                if (!string.IsNullOrEmpty(content))
                {
                    sections.Add((title, content));
                }
            }

            // Debug: log what sections we found
            System.Diagnostics.Debug.WriteLine($"Found {sections.Count} sections:");
            foreach (var section in sections)
            {
                System.Diagnostics.Debug.WriteLine($"- '{section.title}' ({section.content.Length} chars)");
            }

            if (sections.Count == 0)
            {
                sections.Add(("All Hardware Info", allData));
            }
        }

        private void CreateSidebarButtons()
        {
            sidebarPanel.Controls.Clear();
            sectionButtons.Clear();

            // Add title to sidebar
            var titleLabel = new Label
            {
                Location = new Point(20, 20),
                Size = new Size(240, 30),
                Text = "Hardware Sections",
                Font = MonitorDpiPresets.CreatePresetFont(this, "Segoe UI", 12f, FontStyle.Bold),
                ForeColor = Color.FromArgb(220, 220, 220),
                BackColor = Color.Transparent
            };
            sidebarPanel.Controls.Add(titleLabel);

            // Create elegant section buttons
            for (int i = 0; i < sections.Count; i++)
            {
                var sectionButton = CreateSectionButton(sections[i].title, i);
                sectionButtons.Add(sectionButton);
                sidebarPanel.Controls.Add(sectionButton);
            }
        }

        private Button CreateSectionButton(string title, int index)
        {
            // Get appropriate icon for section
            string icon = GetSectionIcon(title);
            
            var button = new Button
            {
                Location = new Point(10, 60 + (index * 50)),
                Size = new Size(260, 45),
                Text = $"{icon} {title}",
                BackColor = Color.FromArgb(50, 50, 50),
                ForeColor = Color.FromArgb(200, 200, 200),
                FlatStyle = FlatStyle.Flat,
                Font = MonitorDpiPresets.CreatePresetFont(this, "Segoe UI", 10f),
                TextAlign = ContentAlignment.MiddleLeft,
                Padding = new Padding(15, 0, 0, 0),
                Cursor = Cursors.Hand,
                Tag = index
            };

            button.FlatAppearance.BorderSize = 0;
            button.FlatAppearance.MouseOverBackColor = Color.FromArgb(70, 70, 70);
            button.FlatAppearance.MouseDownBackColor = Color.FromArgb(0, 120, 215);

            button.Click += (s, e) => {
                ShowSection((int)button.Tag);
                HighlightActiveButton(button);
            };

            return button;
        }

        private string GetSectionIcon(string title)
        {
            var titleLower = title.ToLower();
            if (titleLower.Contains("cpu") || titleLower.Contains("processor")) return "🖥️";
            if (titleLower.Contains("gpu") || titleLower.Contains("graphics")) return "🎮";
            if (titleLower.Contains("ram") || titleLower.Contains("memory")) return "💾";
            if (titleLower.Contains("motherboard") || titleLower.Contains("board")) return "🔌";
            if (titleLower.Contains("disk") || titleLower.Contains("drive") || titleLower.Contains("storage")) return "💿";
            if (titleLower.Contains("network") || titleLower.Contains("ethernet")) return "🌐";
            if (titleLower.Contains("system") || titleLower.Contains("computer")) return "💻";
            if (titleLower.Contains("bios") || titleLower.Contains("firmware")) return "⚙️";
            if (titleLower.Contains("tpm") || titleLower.Contains("security")) return "🔒";
            if (titleLower.Contains("usb") || titleLower.Contains("device")) return "🔌";
            if (titleLower.Contains("monitor") || titleLower.Contains("display")) return "🖥️";
            if (titleLower.Contains("arp") || titleLower.Contains("address")) return "📡";
            return "📋";
        }

        private void HighlightActiveButton(Button activeButton)
        {
            // Reset all buttons
            foreach (var btn in sectionButtons)
            {
                btn.BackColor = Color.FromArgb(50, 50, 50);
                btn.ForeColor = Color.FromArgb(200, 200, 200);
            }

            // Highlight active button
            activeButton.BackColor = Color.FromArgb(0, 120, 215);
            activeButton.ForeColor = Color.White;
        }

        private void ShowSection(int index)
        {
            if (index >= 0 && index < sections.Count)
            {
                currentContentTextBox.Text = sections[index].content;
                currentContentTextBox.SelectionStart = 0;
                currentContentTextBox.ScrollToCaret();
            }
        }

        private void CreateTestSection()
        {
            sections.Add(("Test Section", "This is a test section to verify the modern UI is working.\n\nThe elegant sidebar and content area should be visible."));
            CreateSidebarButtons();
            ShowSection(0);
        }

        private async Task LoadHardwareDataAsync()
        {
            try
            {
                // Show loading indicator
                ShowLoading(true);
                
                // Collect all hardware info using the same method as MainFormLoader
                var allContentBuilder = new System.Text.StringBuilder();
                var results = new string[hardwareInfoManager.GetProviderCount()];
                
                var progress = new Progress<(int index, string content)>(update =>
                {
                    results[update.index] = update.content;
                    var currentResults = results.Where(r => r != null);
                    var combinedContent = string.Join(string.Empty, currentResults);
                    
                    // Update UI on main thread
                    if (this.InvokeRequired)
                    {
                        this.Invoke(new Action(() => {
                            currentHardwareData = combinedContent;
                        }));
                    }
                    else
                    {
                        currentHardwareData = combinedContent;
                    }
                });
                
                // Load hardware data
                await hardwareInfoManager.GetAllHardwareInfo(progress);
                
                // Update UI on main thread
                if (this.InvokeRequired)
                {
                    this.Invoke(new Action(() => {
                        ParseDataIntoSections(currentHardwareData);
                        CreateSidebarButtons();
                        if (sections.Count > 0)
                        {
                            ShowSection(0);
                            if (sectionButtons.Count > 0)
                                HighlightActiveButton(sectionButtons[0]);
                        }
                        ShowLoading(false);
                    }));
                }
                else
                {
                    ParseDataIntoSections(currentHardwareData);
                    CreateSidebarButtons();
                    if (sections.Count > 0)
                    {
                        ShowSection(0);
                        if (sectionButtons.Count > 0)
                            HighlightActiveButton(sectionButtons[0]);
                    }
                    ShowLoading(false);
                }
            }
            catch (Exception ex)
            {
                ShowLoading(false);
                MessageBox.Show($"Error loading hardware information: {ex.Message}", "Error", MessageBoxButtons.OK, MessageBoxIcon.Error);
            }
        }
        
        private void ShowLoading(bool show)
        {
            if (loadingLabel != null)
            {
                loadingLabel.Visible = show;
                if (show)
                {
                    // Center the loading label
                    loadingLabel.Location = new Point(
                        (ClientSize.Width - loadingLabel.Width) / 2,
                        (ClientSize.Height - loadingLabel.Height) / 2
                    );
                    loadingLabel.BringToFront();
                }
            }
        }

        private async void RefreshButton_Click(object sender, EventArgs e)
        {
            if (isMainWindow)
            {
                await LoadHardwareDataAsync();
                MessageBox.Show("Hardware data refreshed successfully!", "Refresh", MessageBoxButtons.OK, MessageBoxIcon.Information);
            }
            else
            {
                if (!string.IsNullOrEmpty(currentHardwareData))
                {
                    ParseDataIntoSections(currentHardwareData);
                    CreateSidebarButtons();
                    if (sections.Count > 0)
                    {
                        ShowSection(0);
                        if (sectionButtons.Count > 0)
                            HighlightActiveButton(sectionButtons[0]);
                    }
                }
                MessageBox.Show("Data refreshed successfully!", "Refresh", MessageBoxButtons.OK, MessageBoxIcon.Information);
            }
        }

        private void ExportButton_Click(object sender, EventArgs e)
        {
            try
            {
                string contentToExport;
                
                if (sections.Count > 0)
                {
                    var allContent = new System.Text.StringBuilder();
                    foreach (var section in sections)
                    {
                        allContent.AppendLine($"===== {section.title} =====");
                        allContent.AppendLine(section.content);
                        allContent.AppendLine();
                    }
                    contentToExport = allContent.ToString();
                }
                else
                {
                    contentToExport = currentHardwareData ?? "No hardware data available.";
                }

                if (string.IsNullOrEmpty(contentToExport))
                {
                    MessageBox.Show("No data to export.", "Export Error", MessageBoxButtons.OK, MessageBoxIcon.Warning);
                    return;
                }

                var fileExportService = new FileExportService(AppDomain.CurrentDomain.BaseDirectory);
                var filePath = fileExportService.ExportHardwareInfo(contentToExport);
                MessageBox.Show($"Export completed successfully!\nSaved to: {filePath}", "Export", MessageBoxButtons.OK, MessageBoxIcon.Information);
            }
            catch (Exception ex)
            {
                MessageBox.Show($"Error exporting file: {ex.Message}", "Export Error", MessageBoxButtons.OK, MessageBoxIcon.Error);
            }
        }
        
        private void CleanDevicesButton_Click(object sender, EventArgs e)
        {
            try
            {
                var cleanDevicesForm = new CleanDevicesForm();
                cleanDevicesForm.ShowDialog();
            }
            catch (Exception ex)
            {
                MessageBox.Show($"Error opening device cleaning: {ex.Message}", "Device Cleaning Error", MessageBoxButtons.OK, MessageBoxIcon.Error);
            }
        }
        
        private void CleanLogsButton_Click(object sender, EventArgs e)
        {
            try
            {
                var cleanLogsForm = new CleanLogsForm();
                cleanLogsForm.ShowDialog();
            }
            catch (Exception ex)
            {
                MessageBox.Show($"Error opening log cleaning: {ex.Message}", "Log Cleaning Error", MessageBoxButtons.OK, MessageBoxIcon.Error);
            }
        }
        
        private async void CheckUpdatesButton_Click(object sender, EventArgs e)
        {
            var button = sender as Button;
            if (button != null)
            {
                button.Enabled = false;
                button.Text = "⟳ Checking...";
            }

            try
            {
                var autoUpdateService = new AutoUpdateService();
                var updateResult = await autoUpdateService.CheckForUpdatesAsync();
                
                switch (updateResult)
                {
                    case UpdateResult.NoUpdateAvailable:
                        MessageBox.Show("You are already running the latest version.", "No Updates Available",
                                       MessageBoxButtons.OK, MessageBoxIcon.Information);
                        break;
                        
                    case UpdateResult.UserDeclined:
                        // User chose not to update - don't show any message
                        break;
                        
                    case UpdateResult.UpdateCompleted:
                        // App will restart automatically - this case shouldn't be reached
                        break;
                        
                    case UpdateResult.Error:
                        // Error message already shown in AutoUpdateService
                        break;
                }
            }
            catch (Exception ex)
            {
                MessageBox.Show($"Error checking for updates: {ex.Message}", "Update Check Failed",
                               MessageBoxButtons.OK, MessageBoxIcon.Warning);
            }
            finally
            {
                if (button != null)
                {
                    button.Enabled = true;
                    button.Text = "⟳ Updates";
                }
            }
        }
        
        private string GetAllContentForExport()
        {
            if (sections.Count > 0)
            {
                var allContent = new System.Text.StringBuilder();
                foreach (var section in sections)
                {
                    allContent.AppendLine($"===== {section.title} =====");
                    allContent.AppendLine(section.content);
                    allContent.AppendLine();
                }
                return allContent.ToString();
            }
            return currentHardwareData ?? "";
        }
    }
}