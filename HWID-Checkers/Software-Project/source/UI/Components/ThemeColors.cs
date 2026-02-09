using System.Drawing;

namespace HWIDChecker.UI.Components
{
    public static class ThemeColors
    {
        // Main background colors
        public static readonly Color MainBackground = Color.FromArgb(30, 30, 30);
        public static readonly Color SecondaryBackground = Color.FromArgb(35, 35, 35);
        public static readonly Color ContentBackground = Color.FromArgb(45, 45, 45);
        public static readonly Color SurfaceBackground = Color.FromArgb(26, 26, 26);
        public static readonly Color BorderSubtle = Color.FromArgb(58, 58, 62);
        public static readonly Color SidebarBackground = Color.FromArgb(34, 34, 36);
        public static readonly Color SidebarItemBackground = Color.FromArgb(46, 46, 50);
        public static readonly Color SidebarItemHover = Color.FromArgb(60, 60, 66);
        public static readonly Color SidebarItemActive = Color.FromArgb(0, 120, 215);
        public static readonly Color SidebarItemText = Color.FromArgb(210, 210, 210);
        public static readonly Color SidebarItemActiveText = Color.White;
        public static readonly Color SidebarHeaderText = Color.FromArgb(235, 235, 235);
        public static readonly Color MutedText = Color.FromArgb(165, 165, 170);
        public static readonly Color SuccessText = Color.FromArgb(125, 205, 125);

        // Button colors
        public static readonly Color ButtonBackground = Color.FromArgb(45, 45, 48);
        public static readonly Color ButtonHover = Color.FromArgb(62, 62, 66);
        public static readonly Color ButtonBorder = Color.FromArgb(80, 80, 83);
        public static readonly Color PrimaryButton = Color.FromArgb(0, 120, 215);
        public static readonly Color PrimaryButtonHover = Color.FromArgb(0, 140, 230);
        public static readonly Color PrimaryButtonPressed = Color.FromArgb(0, 102, 184);
        public static readonly Color DisabledButton = Color.FromArgb(74, 74, 76);
        public static readonly Color DisabledText = Color.FromArgb(160, 160, 160);
        public static readonly Color DangerButton = Color.FromArgb(170, 55, 55);
        public static readonly Color DangerButtonHover = Color.FromArgb(190, 65, 65);

        // Text colors
        public static readonly Color PrimaryText = Color.White;
        public static readonly Color SecondaryText = Color.FromArgb(220, 220, 220);

        // Specific component colors
        public static readonly Color TextBoxBackground = Color.FromArgb(45, 45, 45);
        public static readonly Color TextBoxText = Color.FromArgb(220, 220, 220);
        public static readonly Color ButtonPanelBackground = Color.FromArgb(35, 35, 35);
        public static readonly Color LoadingLabelBackground = Color.FromArgb(45, 45, 45);
        public static readonly Color LoadingLabelText = Color.FromArgb(220, 220, 220);
    }
}
