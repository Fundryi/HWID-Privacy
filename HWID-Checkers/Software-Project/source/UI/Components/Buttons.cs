using System.Drawing;
using System.Windows.Forms;

namespace HWIDChecker.UI.Components
{
    public static class Buttons
    {
        private static readonly Color DefaultBackColor = Color.FromArgb(45, 45, 48);
        private static readonly Color HoverBackColor = Color.FromArgb(62, 62, 66);
        private static readonly Color TextColor = Color.White;
        private static readonly Font ButtonFont = new Font("Segoe UI", 9F, FontStyle.Regular);

        public static void ApplyStyle(Button button)
        {
            button.FlatStyle = FlatStyle.Flat;
            button.FlatAppearance.BorderSize = 0;
            button.BackColor = DefaultBackColor;
            button.ForeColor = TextColor;
            button.Font = ButtonFont;
            button.Padding = new Padding(10, 5, 10, 5);
            button.Margin = new Padding(0, 0, 0, 5);
            button.Cursor = Cursors.Hand;
            button.UseVisualStyleBackColor = false;

            button.MouseEnter += (s, e) =>
            {
                button.BackColor = HoverBackColor;
            };

            button.MouseLeave += (s, e) =>
            {
                button.BackColor = DefaultBackColor;
            };
        }

        // For backwards compatibility with existing code
        public static void ApplyDefaultStyle(Button button) => ApplyStyle(button);
    }
}