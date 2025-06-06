using System;
using System.Collections.Generic;
using System.Drawing;
using System.Linq;
using System.Windows.Forms;

namespace HWIDChecker.Utils
{
    /// <summary>
    /// Pre-calculates and stores perfect DPI settings for each monitor to prevent scaling issues
    /// </summary>
    public static class MonitorDpiPresets
    {
        private static readonly Dictionary<string, MonitorPreset> monitorPresets = new Dictionary<string, MonitorPreset>();
        private static bool presetsInitialized = false;
        
        /// <summary>
        /// Initializes perfect DPI presets for all available monitors
        /// </summary>
        public static void InitializeMonitorPresets()
        {
            if (presetsInitialized) return;
            
            try
            {
                monitorPresets.Clear();
                
                // Get all available screens
                var screens = Screen.AllScreens;
                
                foreach (var screen in screens)
                {
                    var monitorId = GetMonitorId(screen);
                    var dpi = GetMonitorDpi(screen);
                    var preset = CalculateOptimalPreset(dpi, screen.Bounds.Size);
                    
                    monitorPresets[monitorId] = preset;
                    
                    System.Diagnostics.Debug.WriteLine($"Monitor Preset: {monitorId} - DPI: {dpi}, Raw: {preset.RawScale:F2}, Layout: {preset.LayoutScale:F2}, Font: {preset.FontScale:F2}, Size: {preset.ScreenSize.Width}x{preset.ScreenSize.Height}");
                }
                
                presetsInitialized = true;
            }
            catch (Exception ex)
            {
                System.Diagnostics.Debug.WriteLine($"Error initializing monitor presets: {ex.Message}");
                // Create fallback preset
                CreateFallbackPreset();
            }
        }
        
        /// <summary>
        /// Gets the optimal preset for the monitor containing the specified form
        /// </summary>
        public static MonitorPreset GetPresetForForm(Form form)
        {
            if (!presetsInitialized)
                InitializeMonitorPresets();
                
            try
            {
                var screen = Screen.FromControl(form);
                var monitorId = GetMonitorId(screen);
                
                if (monitorPresets.TryGetValue(monitorId, out var preset))
                {
                    return preset;
                }
            }
            catch (Exception ex)
            {
                System.Diagnostics.Debug.WriteLine($"Error getting preset for form: {ex.Message}");
            }
            
            // Return fallback preset
            return GetFallbackPreset();
        }
        
        /// <summary>
        /// Applies the optimal preset to a form based on its current monitor
        /// </summary>
        public static void ApplyOptimalPreset(Form form, int designWidth, int designHeight)
        {
            var preset = GetPresetForForm(form);
            
            // Calculate optimal size using preset
            var optimalSize = new Size(
                (int)Math.Round(designWidth * preset.LayoutScale),
                (int)Math.Round(designHeight * preset.LayoutScale)
            );
            
            // Apply form properties
            form.ClientSize = optimalSize;
            form.MinimumSize = optimalSize;
            form.MaximumSize = optimalSize;
            
            System.Diagnostics.Debug.WriteLine($"Applied preset - Size: {optimalSize}, Layout Scale: {preset.LayoutScale:F2}, Font Scale: {preset.FontScale:F2}");
        }
        
        /// <summary>
        /// Creates a DPI-aware font using the current monitor's preset
        /// </summary>
        public static Font CreatePresetFont(Form form, string fontFamily, float baseSize, FontStyle style = FontStyle.Regular)
        {
            var preset = GetPresetForForm(form);
            var scaledSize = Math.Max(6f, baseSize * preset.FontScale);
            
            try
            {
                return new Font(fontFamily, scaledSize, style);
            }
            catch
            {
                return new Font(FontFamily.GenericSansSerif, scaledSize, style);
            }
        }
        
        /// <summary>
        /// Gets scaled value using the current monitor's preset
        /// </summary>
        public static int GetScaledValue(Form form, int baseValue)
        {
            var preset = GetPresetForForm(form);
            return (int)Math.Round(baseValue * preset.LayoutScale);
        }
        
        /// <summary>
        /// Gets scaled padding using the current monitor's preset
        /// </summary>
        public static Padding GetScaledPadding(Form form, int basePadding)
        {
            var scaledValue = GetScaledValue(form, basePadding);
            return new Padding(scaledValue);
        }
        
        private static string GetMonitorId(Screen screen)
        {
            // Create unique identifier for monitor based on bounds and work area
            return $"{screen.Bounds.Width}x{screen.Bounds.Height}@{screen.Bounds.X},{screen.Bounds.Y}";
        }
        
        private static float GetMonitorDpi(Screen screen)
        {
            try
            {
                // Create a temporary form on this screen to get accurate DPI
                using (var tempForm = new Form())
                {
                    tempForm.StartPosition = FormStartPosition.Manual;
                    tempForm.Location = screen.Bounds.Location;
                    tempForm.WindowState = FormWindowState.Minimized;
                    tempForm.ShowInTaskbar = false;
                    
                    using (var graphics = tempForm.CreateGraphics())
                    {
                        return graphics.DpiX;
                    }
                }
            }
            catch
            {
                // Fallback to system DPI
                return 96f;
            }
        }
        
        private static MonitorPreset CalculateOptimalPreset(float dpi, Size screenSize)
        {
            var rawScale = dpi / 96f;
            
            // NEW GOAL: Visual equivalence to 100% scaling experience
            // The interface should look and feel exactly like 100% scaling with proper space for content
            
            float layoutScale;
            float fontScale;
            
            // Calculate effective screen resolution available for our content
            var effectiveWidth = screenSize.Width / rawScale;  // Available logical pixels
            var effectiveHeight = screenSize.Height / rawScale;
            
            // Target: Ensure we have at least the same visual space as 1920x1080 @ 100%
            var targetMinWidth = 1920f;
            var targetMinHeight = 1080f;
            
            // Calculate scaling needed to provide adequate visual space
            var spaceRatioX = effectiveWidth / targetMinWidth;
            var spaceRatioY = effectiveHeight / targetMinHeight;
            var availableSpaceRatio = Math.Min(spaceRatioX, spaceRatioY);
            
            // Base scaling strategy: aim for visual equivalence
            if (availableSpaceRatio >= 2.0f) // Plenty of space (4K, ultrawide, etc.)
            {
                // Use generous scaling to take advantage of space
                layoutScale = rawScale * 1.4f;
                fontScale = rawScale * 1.3f;
                System.Diagnostics.Debug.WriteLine($"Generous Space: Layout {layoutScale:F2}, Font {fontScale:F2}");
            }
            else if (availableSpaceRatio >= 1.5f) // Good amount of space
            {
                // Use enhanced scaling for better utilization
                layoutScale = rawScale * 1.25f;
                fontScale = rawScale * 1.15f;
                System.Diagnostics.Debug.WriteLine($"Good Space: Layout {layoutScale:F2}, Font {fontScale:F2}");
            }
            else if (availableSpaceRatio >= 1.2f) // Adequate space
            {
                // Use moderate scaling
                layoutScale = rawScale * 1.1f;
                fontScale = rawScale * 1.05f;
                System.Diagnostics.Debug.WriteLine($"Adequate Space: Layout {layoutScale:F2}, Font {fontScale:F2}");
            }
            else if (availableSpaceRatio >= 1.0f) // Just enough space
            {
                // Use standard scaling
                layoutScale = rawScale;
                fontScale = rawScale;
                System.Diagnostics.Debug.WriteLine($"Standard Space: Layout {layoutScale:F2}, Font {fontScale:F2}");
            }
            else // Limited space
            {
                // Use conservative scaling to fit content
                layoutScale = rawScale * 0.9f;
                fontScale = rawScale * 0.95f;
                System.Diagnostics.Debug.WriteLine($"Limited Space: Layout {layoutScale:F2}, Font {fontScale:F2}");
            }
            
            // Ensure minimum usability
            layoutScale = Math.Max(1.0f, layoutScale);
            fontScale = Math.Max(1.0f, fontScale);
            
            // Cap maximum scaling to prevent oversized interfaces
            layoutScale = Math.Min(3.0f, layoutScale);
            fontScale = Math.Min(2.5f, fontScale);
            
            return new MonitorPreset
            {
                Dpi = dpi,
                RawScale = rawScale,
                LayoutScale = layoutScale,
                FontScale = fontScale,
                ScreenSize = screenSize,
                PixelDensity = dpi,
                AvailableSpaceRatio = availableSpaceRatio,
                EffectiveResolution = new Size((int)effectiveWidth, (int)effectiveHeight),
                IsOptimized = true
            };
        }
        
        private static void CreateFallbackPreset()
        {
            monitorPresets["fallback"] = new MonitorPreset
            {
                Dpi = 96f,
                RawScale = 1.0f,
                LayoutScale = 1.0f,
                FontScale = 1.0f,
                ScreenSize = new Size(1920, 1080),
                PixelDensity = 96f,
                AvailableSpaceRatio = 1.0f,
                EffectiveResolution = new Size(1920, 1080),
                IsOptimized = false
            };
        }
        
        private static MonitorPreset GetFallbackPreset()
        {
            if (monitorPresets.TryGetValue("fallback", out var preset))
                return preset;
                
            return new MonitorPreset
            {
                Dpi = 96f,
                RawScale = 1.0f,
                LayoutScale = 1.0f,
                FontScale = 1.0f,
                ScreenSize = new Size(1920, 1080),
                PixelDensity = 96f,
                AvailableSpaceRatio = 1.0f,
                EffectiveResolution = new Size(1920, 1080),
                IsOptimized = false
            };
        }
        
        /// <summary>
        /// Forces re-initialization of monitor presets (useful if display setup changes)
        /// </summary>
        public static void RefreshPresets()
        {
            presetsInitialized = false;
            InitializeMonitorPresets();
        }
        
        /// <summary>
        /// Gets debug information about all monitor presets
        /// </summary>
        public static string GetDebugInfo()
        {
            if (!presetsInitialized)
                InitializeMonitorPresets();
                
            var info = new System.Text.StringBuilder();
            info.AppendLine("Monitor DPI Presets:");
            
            foreach (var kvp in monitorPresets)
            {
                var preset = kvp.Value;
                info.AppendLine($"  {kvp.Key}:");
                info.AppendLine($"    DPI: {preset.Dpi:F1}");
                info.AppendLine($"    Raw Scale: {preset.RawScale:F2}");
                info.AppendLine($"    Layout Scale: {preset.LayoutScale:F2}");
                info.AppendLine($"    Font Scale: {preset.FontScale:F2}");
                info.AppendLine($"    Screen: {preset.ScreenSize.Width}x{preset.ScreenSize.Height}");
                info.AppendLine($"    Effective Resolution: {preset.EffectiveResolution.Width}x{preset.EffectiveResolution.Height}");
                info.AppendLine($"    Space Ratio: {preset.AvailableSpaceRatio:F2}");
                info.AppendLine($"    Visual Size (920x680): {preset.GetVisualSize(920, 680).Width}x{preset.GetVisualSize(920, 680).Height}");
                info.AppendLine($"    Space Efficiency: {preset.GetSpaceEfficiency():F2}");
                info.AppendLine($"    Optimized: {preset.IsOptimized}");
            }
            
            return info.ToString();
        }
    }
    
    /// <summary>
    /// Represents optimized DPI settings for a specific monitor
    /// </summary>
    public struct MonitorPreset
    {
        public float Dpi { get; set; }
        public float RawScale { get; set; }
        public float LayoutScale { get; set; }
        public float FontScale { get; set; }
        public Size ScreenSize { get; set; }
        public float PixelDensity { get; set; }
        public float AvailableSpaceRatio { get; set; }
        public Size EffectiveResolution { get; set; }
        public bool IsOptimized { get; set; }
        
        /// <summary>
        /// Gets the visual window size this preset will produce for a given design size
        /// </summary>
        public Size GetVisualSize(int designWidth, int designHeight)
        {
            return new Size(
                (int)Math.Round(designWidth * LayoutScale),
                (int)Math.Round(designHeight * LayoutScale)
            );
        }
        
        /// <summary>
        /// Gets the visual space efficiency compared to 100% baseline
        /// </summary>
        public float GetSpaceEfficiency()
        {
            return AvailableSpaceRatio * LayoutScale;
        }
    }
}