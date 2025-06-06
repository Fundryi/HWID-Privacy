using System;
using System.Collections.Generic;
using System.Drawing;
using System.Windows.Forms;

namespace HWIDChecker.Utils
{
    /// <summary>
    /// Monitor-locked DPI scaling - each monitor gets fixed, perfect settings
    /// </summary>
    public static class StandardDpiScaling
    {
        private static readonly Dictionary<string, MonitorSettings> monitorSettings = new Dictionary<string, MonitorSettings>();
        private static bool initialized = false;
        
        /// <summary>
        /// Initialize monitor-specific settings at startup
        /// </summary>
        public static void Initialize()
        {
            if (initialized) return;
            
            var screens = Screen.AllScreens;
            foreach (var screen in screens)
            {
                var id = GetMonitorId(screen);
                var dpi = GetMonitorDpi(screen);
                var settings = CalculateSettingsForMonitor(dpi, screen.Bounds.Size);
                monitorSettings[id] = settings;
                
                System.Diagnostics.Debug.WriteLine($"Monitor {id}: DPI {dpi}, Scale {settings.Scale:F2}");
            }
            
            initialized = true;
        }
        
        /// <summary>
        /// Configure form with monitor-locked scaling
        /// </summary>
        public static void ConfigureForm(Form form)
        {
            if (!initialized) Initialize();
            
            // Disable Windows automatic scaling - we'll handle it manually
            form.AutoScaleMode = AutoScaleMode.None;
            form.FormBorderStyle = FormBorderStyle.FixedSingle;
            
            ApplyMonitorSettings(form, 920, 680);
        }
        
        /// <summary>
        /// Set form size using monitor-specific scaling
        /// </summary>
        public static void SetLogicalSize(Form form, int baseWidth, int baseHeight)
        {
            ApplyMonitorSettings(form, baseWidth, baseHeight);
        }
        
        /// <summary>
        /// Apply the correct settings for the monitor containing this form
        /// </summary>
        private static void ApplyMonitorSettings(Form form, int baseWidth, int baseHeight)
        {
            var screen = Screen.FromControl(form);
            var id = GetMonitorId(screen);
            
            if (monitorSettings.TryGetValue(id, out var settings))
            {
                var scaledSize = new Size(
                    (int)(baseWidth * settings.Scale),
                    (int)(baseHeight * settings.Scale)
                );
                
                form.ClientSize = scaledSize;
                form.MinimumSize = scaledSize;
                form.MaximumSize = scaledSize;
                
                System.Diagnostics.Debug.WriteLine($"Applied settings for {id}: {scaledSize} (scale {settings.Scale:F2})");
            }
        }
        
        /// <summary>
        /// Create font with monitor-specific scaling
        /// </summary>
        public static Font CreateLogicalFont(string familyName, float baseSize, FontStyle style = FontStyle.Regular)
        {
            // For now, use base size - we can enhance this later if needed
            try
            {
                return new Font(familyName, baseSize, style);
            }
            catch
            {
                return new Font(FontFamily.GenericSansSerif, baseSize, style);
            }
        }
        
        /// <summary>
        /// Create other UI elements (keep simple for now)
        /// </summary>
        public static Padding CreateLogicalPadding(int padding) => new Padding(padding);
        public static Size CreateLogicalSize(int width, int height) => new Size(width, height);
        public static Point CreateLogicalPoint(int x, int y) => new Point(x, y);
        
        /// <summary>
        /// Handle DPI changes by applying monitor-specific settings
        /// </summary>
        public static void HandleDpiChange(Form form, ref Message m)
        {
            const int WM_DPICHANGED = 0x02E0;
            if (m.Msg == WM_DPICHANGED)
            {
                // Get suggested position from Windows
                var rect = (RECT)System.Runtime.InteropServices.Marshal.PtrToStructure(m.LParam, typeof(RECT));
                
                // Move to new position
                form.Location = new Point(rect.left, rect.top);
                
                // Apply our locked settings for the new monitor
                ApplyMonitorSettings(form, 920, 680);
                
                System.Diagnostics.Debug.WriteLine($"DPI change handled - moved to new monitor");
            }
        }
        
        private static string GetMonitorId(Screen screen)
        {
            return $"{screen.Bounds.Width}x{screen.Bounds.Height}@{screen.Bounds.X},{screen.Bounds.Y}";
        }
        
        private static float GetMonitorDpi(Screen screen)
        {
            try
            {
                using (var form = new Form())
                {
                    form.StartPosition = FormStartPosition.Manual;
                    form.Location = screen.Bounds.Location;
                    form.WindowState = FormWindowState.Minimized;
                    form.ShowInTaskbar = false;
                    
                    using (var graphics = form.CreateGraphics())
                    {
                        return graphics.DpiX;
                    }
                }
            }
            catch
            {
                return 96f;
            }
        }
        
        private static MonitorSettings CalculateSettingsForMonitor(float dpi, Size screenSize)
        {
            // GOAL: Make the interface look good and usable on each specific monitor
            // NOT physical size matching - visual consistency and usability
            
            var systemScale = dpi / 96f; // What Windows thinks the scaling should be
            var screenArea = screenSize.Width * screenSize.Height;
            
            float interfaceScale;
            
            // Simple rules based on what looks good on each type of display
            if (screenArea >= 8000000) // 4K displays (3840x2160+)
            {
                // 4K needs larger interface to be usable and look proportional
                if (systemScale >= 1.5f) interfaceScale = 2.2f; // 4K@150% → generous interface
                else if (systemScale >= 1.25f) interfaceScale = 1.8f; // 4K@125% → enhanced
                else interfaceScale = 1.5f; // 4K@100% → still enhanced
            }
            else if (screenArea >= 3600000) // 1440p displays (2560x1440+)
            {
                // 1440p gets moderate enhancement
                if (systemScale >= 1.5f) interfaceScale = 1.7f; // 1440p@150% → enhanced
                else if (systemScale >= 1.25f) interfaceScale = 1.4f; // 1440p@125% → moderate
                else interfaceScale = 1.2f; // 1440p@100% → slight enhancement
            }
            else // 1080p and smaller displays
            {
                // Standard displays use close to system scaling
                if (systemScale >= 1.5f) interfaceScale = 1.5f; // 1080p@150% → standard
                else if (systemScale >= 1.25f) interfaceScale = 1.25f; // 1080p@125% → standard
                else interfaceScale = 1.0f; // 1080p@100% → baseline
            }
            
            return new MonitorSettings { Scale = interfaceScale, Dpi = dpi };
        }
        
        private struct MonitorSettings
        {
            public float Scale;
            public float Dpi;
        }
        
        [System.Runtime.InteropServices.StructLayout(System.Runtime.InteropServices.LayoutKind.Sequential)]
        private struct RECT
        {
            public int left, top, right, bottom;
        }
    }
}