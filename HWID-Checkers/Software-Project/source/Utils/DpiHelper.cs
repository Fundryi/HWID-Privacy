using System.Drawing;

namespace HWIDChecker.Utils
{
    /// <summary>
    /// Simple helper for DPI-related operations that work with Windows Forms automatic scaling
    /// </summary>
    public static class DpiHelper
    {
        /// <summary>
        /// Creates a font that will be automatically scaled by Windows Forms
        /// </summary>
        public static Font CreateFont(string familyName, float size, FontStyle style = FontStyle.Regular)
        {
            return new Font(familyName, size, style);
        }
        
        /// <summary>
        /// Returns the recommended base size for forms - Windows will handle scaling
        /// </summary>
        public static Size GetBaseFormSize(int width, int height)
        {
            return new Size(width, height);
        }
        
        /// <summary>
        /// Creates standard padding that Windows Forms will scale automatically
        /// </summary>
        public static Padding CreatePadding(int all)
        {
            return new Padding(all);
        }
        
        /// <summary>
        /// Creates standard margins that Windows Forms will scale automatically
        /// </summary>
        public static Padding CreateMargin(int all)
        {
            return new Padding(all);
        }
    }
}