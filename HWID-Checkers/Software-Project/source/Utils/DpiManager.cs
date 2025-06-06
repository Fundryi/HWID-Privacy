using System;
using System.Drawing;
using System.Windows.Forms;

namespace HWIDChecker.Utils
{
    /// <summary>
    /// Centralized DPI management to prevent scaling issues when switching between monitors
    /// </summary>
    public static class DpiManager
    {
        private static readonly object lockObject = new object();
        private static float? cachedSystemDpi = null;
        private static DateTime lastDpiCheck = DateTime.MinValue;
        private static readonly TimeSpan DPI_CACHE_DURATION = TimeSpan.FromMilliseconds(500);
        
        /// <summary>
        /// Gets the current system DPI in a thread-safe manner with caching to prevent scaling issues
        /// </summary>
        public static float GetSystemDpi()
        {
            lock (lockObject)
            {
                // Use cached value if recent to prevent excessive DPI queries
                if (cachedSystemDpi.HasValue && DateTime.Now - lastDpiCheck < DPI_CACHE_DURATION)
                {
                    return cachedSystemDpi.Value;
                }
                
                try
                {
                    using (var form = new Form())
                    using (var graphics = form.CreateGraphics())
                    {
                        cachedSystemDpi = graphics.DpiX;
                        lastDpiCheck = DateTime.Now;
                        return cachedSystemDpi.Value;
                    }
                }
                catch
                {
                    // Fallback to standard DPI if detection fails
                    cachedSystemDpi = 96f;
                    lastDpiCheck = DateTime.Now;
                    return 96f;
                }
            }
        }
        
        /// <summary>
        /// Gets the scaling factor for the current system DPI
        /// </summary>
        public static float GetScalingFactor()
        {
            return GetSystemDpi() / 96f;
        }
        
        /// <summary>
        /// Calculates DPI-aware size for a form with stabilized scaling
        /// </summary>
        public static Size GetDpiAwareSize(int designWidth, int designHeight)
        {
            var scalingFactor = GetScalingFactor();
            
            // Apply conservative scaling to prevent oversized windows
            var adjustedScaling = GetConservativeScaling(scalingFactor);
            
            int scaledWidth = (int)Math.Round(designWidth * adjustedScaling);
            int scaledHeight = (int)Math.Round(designHeight * adjustedScaling);
            
            return new Size(scaledWidth, scaledHeight);
        }
        
        /// <summary>
        /// Applies conservative scaling to prevent unusably large windows
        /// </summary>
        private static float GetConservativeScaling(float rawScaling)
        {
            // Apply the same smart scaling logic from the DPI documentation
            if (rawScaling <= 1.25f)
                return Math.Max(1.0f, rawScaling * 0.9f); // 125% becomes ~112%
            else if (rawScaling <= 1.5f)
                return rawScaling * 0.8f; // 150% becomes ~120%
            else
                return 1.3f; // Cap at 130% for very high DPI
        }
        
        /// <summary>
        /// Configures a form for proper DPI handling without scaling issues
        /// </summary>
        public static void ConfigureFormForDpi(Form form, int designWidth, int designHeight)
        {
            if (form == null) return;
            
            // Set form properties to prevent automatic scaling issues
            form.AutoScaleMode = AutoScaleMode.Dpi;
            form.FormBorderStyle = FormBorderStyle.FixedSingle;
            
            // Get stable DPI-aware size
            var dpiSize = GetDpiAwareSize(designWidth, designHeight);
            
            // Apply size constraints to prevent resizing and scaling drift
            form.ClientSize = dpiSize;
            form.MinimumSize = dpiSize;
            form.MaximumSize = dpiSize;
            
            // Don't apply scaling initially - let the form handle it
        }
        
        /// <summary>
        /// Handles DPI changes in a controlled manner to prevent scaling accumulation
        /// </summary>
        public static void HandleDpiChange(Form form, int designWidth, int designHeight, ref Message m)
        {
            const int WM_DPICHANGED = 0x02E0;
            if (m.Msg == WM_DPICHANGED)
            {
                try
                {
                    // Clear the DPI cache to force fresh calculation
                    InvalidateDpiCache();
                    
                    // Use Windows' suggested rectangle but with our conservative scaling
                    var suggestedRect = (RECT)System.Runtime.InteropServices.Marshal.PtrToStructure(m.LParam, typeof(RECT));
                    
                    // Calculate our own conservative size instead of using Windows' suggestion
                    var conservativeSize = GetDpiAwareSize(designWidth, designHeight);
                    
                    // Position the window using Windows' suggestion but with our size
                    form.SetBounds(
                        suggestedRect.left,
                        suggestedRect.top,
                        conservativeSize.Width,
                        conservativeSize.Height
                    );
                    
                    // Ensure size constraints remain in place
                    form.MinimumSize = conservativeSize;
                    form.MaximumSize = conservativeSize;
                    
                    // Controls will be handled by the form's own DPI logic
                }
                catch (Exception ex)
                {
                    System.Diagnostics.Debug.WriteLine($"DPI change handling error: {ex.Message}");
                    // If handling fails, just apply our standard DPI configuration
                    ConfigureFormForDpi(form, designWidth, designHeight);
                }
            }
        }
        
        /// <summary>
        /// Invalidates the DPI cache to force fresh calculation
        /// </summary>
        public static void InvalidateDpiCache()
        {
            lock (lockObject)
            {
                cachedSystemDpi = null;
                lastDpiCheck = DateTime.MinValue;
            }
        }
        
        /// <summary>
        /// Creates a DPI-aware font with conservative scaling
        /// </summary>
        public static Font CreateDpiAwareFont(string fontFamily, float baseSize, FontStyle style = FontStyle.Regular)
        {
            var scalingFactor = GetScalingFactor();
            var fontScaling = GetConservativeFontScaling(scalingFactor);
            var scaledSize = Math.Max(6f, baseSize * fontScaling); // Minimum 6pt font
            
            try
            {
                return new Font(fontFamily, scaledSize, style);
            }
            catch
            {
                // Fallback to default font if requested font fails
                return new Font(FontFamily.GenericSansSerif, scaledSize, style);
            }
        }
        
        /// <summary>
        /// Applies conservative font scaling
        /// </summary>
        private static float GetConservativeFontScaling(float rawScaling)
        {
            if (rawScaling <= 1.5f)
                return Math.Max(1.0f, rawScaling * 0.85f); // More conservative font scaling
            else
                return 1.2f; // Cap at 120% for fonts
        }
        
        /// <summary>
        /// Applies DPI scaling to all controls recursively
        /// </summary>
        private static void ApplyDpiScalingToControls(Control parent)
        {
            if (parent == null) return;
            
            var scalingFactor = GetScalingFactor();
            var conservativeScaling = GetConservativeScaling(scalingFactor);
            var fontScaling = GetConservativeFontScaling(scalingFactor);
            
            // Apply scaling to all controls recursively
            ApplyScalingRecursive(parent, conservativeScaling, fontScaling);
        }
        
        /// <summary>
        /// Recursively applies scaling to controls and their children
        /// </summary>
        private static void ApplyScalingRecursive(Control control, float layoutScaling, float fontScaling)
        {
            if (control == null) return;
            
            try
            {
                // Skip the form itself to avoid double scaling
                if (control is Form)
                {
                    // Only process child controls of the form
                    foreach (Control child in control.Controls)
                    {
                        ApplyScalingRecursive(child, layoutScaling, fontScaling);
                    }
                    return;
                }
                
                // Scale control size and position (except for docked controls)
                if (control.Dock == DockStyle.None)
                {
                    var newLocation = new Point(
                        (int)Math.Round(control.Location.X * layoutScaling),
                        (int)Math.Round(control.Location.Y * layoutScaling)
                    );
                    var newSize = new Size(
                        (int)Math.Round(control.Size.Width * layoutScaling),
                        (int)Math.Round(control.Size.Height * layoutScaling)
                    );
                    
                    control.Location = newLocation;
                    control.Size = newSize;
                }
                else if (control.Dock == DockStyle.Bottom || control.Dock == DockStyle.Top)
                {
                    // For docked controls, only scale height
                    var newHeight = (int)Math.Round(control.Height * layoutScaling);
                    if (newHeight > 0) control.Height = newHeight;
                }
                
                // Scale font if control has one
                if (control.Font != null)
                {
                    var originalSize = control.Font.Size;
                    var scaledSize = Math.Max(6f, originalSize * fontScaling);
                    
                    try
                    {
                        var newFont = new Font(control.Font.FontFamily, scaledSize, control.Font.Style);
                        control.Font = newFont;
                    }
                    catch
                    {
                        // If font creation fails, keep original font
                    }
                }
                
                // Scale padding and margin carefully
                if (control.Padding != Padding.Empty)
                {
                    control.Padding = new Padding(
                        Math.Max(0, (int)Math.Round(control.Padding.Left * layoutScaling)),
                        Math.Max(0, (int)Math.Round(control.Padding.Top * layoutScaling)),
                        Math.Max(0, (int)Math.Round(control.Padding.Right * layoutScaling)),
                        Math.Max(0, (int)Math.Round(control.Padding.Bottom * layoutScaling))
                    );
                }
                
                // Recursively apply to child controls
                foreach (Control child in control.Controls)
                {
                    ApplyScalingRecursive(child, layoutScaling, fontScaling);
                }
            }
            catch (Exception ex)
            {
                System.Diagnostics.Debug.WriteLine($"Error scaling control {control.Name}: {ex.Message}");
            }
        }
        
        [System.Runtime.InteropServices.StructLayout(System.Runtime.InteropServices.LayoutKind.Sequential)]
        private struct RECT
        {
            public int left, top, right, bottom;
        }
    }
}