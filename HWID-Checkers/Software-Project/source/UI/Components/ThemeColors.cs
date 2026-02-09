using System.Drawing;

namespace HWIDChecker.UI.Components
{
    public static class ThemeColors
    {
        // Main background colors
        public static readonly Color MainBackground = Color.FromArgb(30, 30, 30);
        public static readonly Color SecondaryBackground = Color.FromArgb(35, 35, 35);
        public static readonly Color ContentBackground = Color.FromArgb(45, 45, 45);

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
