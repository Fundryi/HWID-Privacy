using System.Drawing;
using System.Windows.Forms;

namespace HWIDChecker.UI.Components
{
    public static class Buttons
    {
        private static readonly Font ButtonFont = new Font("Segoe UI", 9F, FontStyle.Regular);

        public enum ButtonVariant
        {
            Primary,
            Secondary,
            Danger
        }

        public static void ApplyStyle(Button button, ButtonVariant variant = ButtonVariant.Secondary)
        {
            var normalColor = GetNormalColor(variant);
            var hoverColor = GetHoverColor(variant);
            var pressedColor = GetPressedColor(variant);

            button.FlatStyle = FlatStyle.Flat;
            button.FlatAppearance.BorderSize = 1;
            button.FlatAppearance.BorderColor = ThemeColors.ButtonBorder;
            button.BackColor = normalColor;
            button.ForeColor = ThemeColors.PrimaryText;
            button.Font = ButtonFont;
            button.Padding = new Padding(10, 5, 10, 5);
            button.Cursor = Cursors.Hand;
            button.UseVisualStyleBackColor = false;

            button.MouseEnter += (s, e) =>
            {
                if (button.Enabled)
                {
                    button.BackColor = hoverColor;
                }
            };

            button.MouseLeave += (s, e) =>
            {
                if (button.Enabled)
                {
                    button.BackColor = normalColor;
                }
            };

            button.MouseDown += (s, e) =>
            {
                if (button.Enabled && e.Button == MouseButtons.Left)
                {
                    button.BackColor = pressedColor;
                }
            };

            button.MouseUp += (s, e) =>
            {
                if (button.Enabled)
                {
                    button.BackColor = hoverColor;
                }
            };

            button.EnabledChanged += (s, e) =>
            {
                if (button.Enabled)
                {
                    button.BackColor = normalColor;
                    button.ForeColor = ThemeColors.PrimaryText;
                }
                else
                {
                    button.BackColor = ThemeColors.DisabledButton;
                    button.ForeColor = ThemeColors.DisabledText;
                }
            };
        }

        private static Color GetNormalColor(ButtonVariant variant)
        {
            return variant switch
            {
                ButtonVariant.Primary => ThemeColors.PrimaryButton,
                ButtonVariant.Danger => ThemeColors.DangerButton,
                _ => ThemeColors.ButtonBackground
            };
        }

        private static Color GetHoverColor(ButtonVariant variant)
        {
            return variant switch
            {
                ButtonVariant.Primary => ThemeColors.PrimaryButtonHover,
                ButtonVariant.Danger => ThemeColors.DangerButtonHover,
                _ => ThemeColors.ButtonHover
            };
        }

        private static Color GetPressedColor(ButtonVariant variant)
        {
            return variant switch
            {
                ButtonVariant.Primary => ThemeColors.PrimaryButtonPressed,
                ButtonVariant.Danger => ThemeColors.DangerButton,
                _ => ThemeColors.ButtonBackground
            };
        }

        // For backwards compatibility with existing code
        public static void ApplyDefaultStyle(Button button) => ApplyStyle(button, ButtonVariant.Secondary);
    }
}
