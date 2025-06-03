using System;
using System.Drawing;
using System.Runtime.InteropServices;
using System.Windows.Forms;

namespace HWIDChecker.Services
{
    public class DpiScalingService
    {
        #region Win32 API Declarations
        
        [DllImport("user32.dll")]
        private static extern IntPtr GetDC(IntPtr hWnd);

        [DllImport("gdi32.dll")]
        private static extern int GetDeviceCaps(IntPtr hdc, int nIndex);

        [DllImport("user32.dll")]
        private static extern int ReleaseDC(IntPtr hWnd, IntPtr hDC);

        [DllImport("shcore.dll")]
        private static extern int GetDpiForMonitor(IntPtr hmonitor, DpiType dpiType, out uint dpiX, out uint dpiY);

        [DllImport("user32.dll")]
        private static extern IntPtr MonitorFromWindow(IntPtr handle, int flags);

        [DllImport("user32.dll")]
        private static extern bool SetProcessDPIAware();

        [DllImport("shcore.dll")]
        private static extern int SetProcessDpiAwareness(ProcessDpiAwareness awareness);

        private const int LOGPIXELSX = 88;
        private const int LOGPIXELSY = 90;
        private const int MONITOR_DEFAULTTONEAREST = 2;

        private enum DpiType
        {
            Effective = 0,
            Angular = 1,
            Raw = 2,
        }

        private enum ProcessDpiAwareness
        {
            Unaware = 0,
            SystemAware = 1,
            PerMonitorAware = 2
        }

        #endregion

        private static DpiScalingService _instance;
        private float _scaleFactor = 1.0f;
        private float _fontScaleFactor = 1.0f;
        private readonly object _lock = new object();
        
        // Smart scaling limits to prevent UI from becoming unusable
        private const float MAX_SCALE_FACTOR = 1.3f;        // Maximum 130% scaling for layout
        private const float MAX_FONT_SCALE_FACTOR = 1.2f;   // Maximum 120% font scaling
        private const float MIN_SCALE_FACTOR = 0.9f;        // Minimum 90% scaling

        public static DpiScalingService Instance
        {
            get
            {
                if (_instance == null)
                {
                    _instance = new DpiScalingService();
                }
                return _instance;
            }
        }

        private DpiScalingService()
        {
            InitializeDpiAwareness();
            CalculateScaleFactor();
        }

        public float ScaleFactor => _scaleFactor;
        public float FontScaleFactor => _fontScaleFactor;

        public float BaseDpi => 96.0f; // Standard Windows DPI

        private void InitializeDpiAwareness()
        {
            try
            {
                // Try to set per-monitor DPI awareness (Windows 8.1+)
                if (Environment.OSVersion.Version.Major > 6 || 
                    (Environment.OSVersion.Version.Major == 6 && Environment.OSVersion.Version.Minor >= 3))
                {
                    SetProcessDpiAwareness(ProcessDpiAwareness.PerMonitorAware);
                }
                else
                {
                    // Fallback for older Windows versions
                    SetProcessDPIAware();
                }
            }
            catch (Exception ex)
            {
                System.Diagnostics.Debug.WriteLine($"Failed to set DPI awareness: {ex.Message}");
                // Continue without DPI awareness - will use system scaling
            }
        }

        private void CalculateScaleFactor()
        {
            try
            {
                IntPtr hdc = GetDC(IntPtr.Zero);
                if (hdc != IntPtr.Zero)
                {
                    int dpiX = GetDeviceCaps(hdc, LOGPIXELSX);
                    float rawScaleFactor = dpiX / BaseDpi;
                    
                    // Apply smart scaling with limits
                    _scaleFactor = CalculateSmartScaleFactor(rawScaleFactor);
                    _fontScaleFactor = CalculateSmartFontScaleFactor(rawScaleFactor);
                    
                    ReleaseDC(IntPtr.Zero, hdc);
                    
                    System.Diagnostics.Debug.WriteLine($"Raw DPI Scale: {rawScaleFactor:F2}x, Smart Scale: {_scaleFactor:F2}x, Font Scale: {_fontScaleFactor:F2}x");
                }
            }
            catch (Exception ex)
            {
                System.Diagnostics.Debug.WriteLine($"Failed to calculate DPI scale factor: {ex.Message}");
                _scaleFactor = 1.0f;
                _fontScaleFactor = 1.0f;
            }
        }

        private float CalculateSmartScaleFactor(float rawScale)
        {
            // For layout scaling, we want to be more conservative
            if (rawScale <= 1.0f)
                return Math.Max(rawScale, MIN_SCALE_FACTOR);
            
            // Progressive scaling reduction for high DPI
            if (rawScale <= 1.25f)
                return Math.Min(rawScale * 0.9f, MAX_SCALE_FACTOR); // Reduce 125% to ~112%
            else if (rawScale <= 1.5f)
                return Math.Min(rawScale * 0.8f, MAX_SCALE_FACTOR); // Reduce 150% to ~120%
            else
                return MAX_SCALE_FACTOR; // Cap at 130% for very high DPI
        }

        private float CalculateSmartFontScaleFactor(float rawScale)
        {
            // For fonts, we can be slightly more aggressive but still capped
            if (rawScale <= 1.0f)
                return Math.Max(rawScale, MIN_SCALE_FACTOR);
            
            // Progressive font scaling
            if (rawScale <= 1.25f)
                return Math.Min(rawScale * 0.95f, MAX_FONT_SCALE_FACTOR); // Reduce 125% to ~119%
            else if (rawScale <= 1.5f)
                return Math.Min(rawScale * 0.85f, MAX_FONT_SCALE_FACTOR); // Reduce 150% to ~127% -> capped to 120%
            else
                return MAX_FONT_SCALE_FACTOR; // Cap at 120% for fonts
        }

        public float GetDpiForWindow(IntPtr windowHandle)
        {
            try
            {
                IntPtr monitor = MonitorFromWindow(windowHandle, MONITOR_DEFAULTTONEAREST);
                if (monitor != IntPtr.Zero)
                {
                    if (GetDpiForMonitor(monitor, DpiType.Effective, out uint dpiX, out uint dpiY) == 0)
                    {
                        float rawScale = dpiX / BaseDpi;
                        return CalculateSmartScaleFactor(rawScale);
                    }
                }
            }
            catch (Exception ex)
            {
                System.Diagnostics.Debug.WriteLine($"Failed to get DPI for window: {ex.Message}");
            }

            return _scaleFactor; // Fallback to system DPI
        }

        public int ScaleValue(int value)
        {
            return (int)Math.Round(value * _scaleFactor);
        }

        public int ScaleValue(int value, float customScale)
        {
            return (int)Math.Round(value * customScale);
        }

        public Size ScaleSize(Size size)
        {
            return new Size(ScaleValue(size.Width), ScaleValue(size.Height));
        }

        public Size ScaleSize(Size size, float customScale)
        {
            return new Size(ScaleValue(size.Width, customScale), ScaleValue(size.Height, customScale));
        }

        public Point ScalePoint(Point point)
        {
            return new Point(ScaleValue(point.X), ScaleValue(point.Y));
        }

        public Padding ScalePadding(Padding padding)
        {
            return new Padding(
                ScaleValue(padding.Left),
                ScaleValue(padding.Top),
                ScaleValue(padding.Right),
                ScaleValue(padding.Bottom)
            );
        }

        public Font ScaleFont(Font font)
        {
            return new Font(font.FontFamily, font.Size * _fontScaleFactor, font.Style);
        }

        public Font ScaleFont(Font font, float customScale)
        {
            // Apply smart scaling to custom scale factors too
            float smartCustomScale = CalculateSmartFontScaleFactor(customScale);
            return new Font(font.FontFamily, font.Size * smartCustomScale, font.Style);
        }

        public void ScaleControl(Control control)
        {
            ScaleControlRecursive(control);
        }

        private void ScaleControlRecursive(Control control)
        {
            if (control == null) return;

            // Don't scale controls that are docked to fill - they will scale automatically
            if (control.Dock != DockStyle.Fill && control.Dock != DockStyle.None)
            {
                // For other dock styles, we may need to adjust specific properties
                if (control is Panel panel && control.Dock == DockStyle.Bottom)
                {
                    panel.Height = ScaleValue(panel.Height);
                }
            }
            else if (control.Dock == DockStyle.None)
            {
                // Scale size and location for non-docked controls
                control.Size = ScaleSize(control.Size);
                control.Location = ScalePoint(control.Location);
            }

            // Scale margins and padding
            control.Margin = ScalePadding(control.Margin);
            control.Padding = ScalePadding(control.Padding);

            // Scale font
            if (control.Font != null)
            {
                var scaledFont = ScaleFont(control.Font);
                control.Font = scaledFont;
            }

            // Scale minimum size if set
            if (!control.MinimumSize.IsEmpty)
            {
                control.MinimumSize = ScaleSize(control.MinimumSize);
            }

            // Scale maximum size if set
            if (!control.MaximumSize.IsEmpty)
            {
                control.MaximumSize = ScaleSize(control.MaximumSize);
            }

            // Recursively scale child controls
            foreach (Control child in control.Controls)
            {
                ScaleControlRecursive(child);
            }
        }

        public void UpdateScaleFactorForWindow(IntPtr windowHandle)
        {
            lock (_lock)
            {
                float newScale = GetDpiForWindow(windowHandle);
                if (Math.Abs(newScale - _scaleFactor) > 0.01f) // Only update if significantly different
                {
                    _scaleFactor = newScale;
                    
                    // Also update font scale factor based on the new DPI
                    try
                    {
                        IntPtr monitor = MonitorFromWindow(windowHandle, MONITOR_DEFAULTTONEAREST);
                        if (monitor != IntPtr.Zero)
                        {
                            if (GetDpiForMonitor(monitor, DpiType.Effective, out uint dpiX, out uint dpiY) == 0)
                            {
                                float rawScale = dpiX / BaseDpi;
                                _fontScaleFactor = CalculateSmartFontScaleFactor(rawScale);
                            }
                        }
                    }
                    catch (Exception ex)
                    {
                        System.Diagnostics.Debug.WriteLine($"Failed to update font scale factor: {ex.Message}");
                    }
                }
            }
        }

        public bool IsHighDpi => _scaleFactor > 1.0f;

        public string GetDpiInfo()
        {
            return $"Layout Scale: {_scaleFactor:F2}x, Font Scale: {_fontScaleFactor:F2}x ({BaseDpi * _scaleFactor:F0} DPI)";
        }

        /// <summary>
        /// Get a conservative scaling factor that's safer for main application windows
        /// </summary>
        public float GetConservativeScaleFactor()
        {
            // Even more conservative scaling for main windows
            if (_scaleFactor <= 1.1f)
                return _scaleFactor;
            else if (_scaleFactor <= 1.2f)
                return 1.1f;  // Cap at 110% for moderate scaling
            else
                return 1.15f; // Cap at 115% for high scaling
        }

        /// <summary>
        /// Scale a size using conservative factors to prevent UI from becoming too large
        /// </summary>
        public Size ScaleSizeConservative(Size size)
        {
            float conservativeScale = GetConservativeScaleFactor();
            return new Size(
                (int)Math.Round(size.Width * conservativeScale),
                (int)Math.Round(size.Height * conservativeScale)
            );
        }

        /// <summary>
        /// Scale a value using conservative factors
        /// </summary>
        public int ScaleValueConservative(int value)
        {
            float conservativeScale = GetConservativeScaleFactor();
            return (int)Math.Round(value * conservativeScale);
        }
    }
}