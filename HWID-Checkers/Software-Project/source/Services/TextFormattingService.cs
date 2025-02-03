using System.Text;

namespace HWIDChecker.Services
{
    public class TextFormattingService
    {
        private const int LINE_WIDTH = 93;
        private const int ITEM_SEPARATOR_WIDTH = 40;
        private const char MAIN_SEPARATOR_CHAR = '=';
        private const char ITEM_SEPARATOR_CHAR = '-';
        private const string PIPE_SEPARATOR = " | ";

        public string FormatHeader(string text)
        {
            var sb = new StringBuilder();
            AppendSeparatorLine(sb);
            AppendCenteredText(sb, text);
            AppendSeparatorLine(sb);
            return sb.ToString();
        }

        public string FormatSection(string title, string content)
        {
            var sb = new StringBuilder();
            AppendSeparatorLine(sb);
            AppendCenteredText(sb, title);
            AppendSeparatorLine(sb);
            if (!content.EndsWith(Environment.NewLine))
            {
                sb.Append(content);
                sb.AppendLine();
            }
            else
            {
                sb.Append(content);
            }
            return sb.ToString();
        }

        private void AppendSeparatorLine(StringBuilder sb)
        {
            sb.AppendLine(new string(MAIN_SEPARATOR_CHAR, LINE_WIDTH));
        }

        public void AppendItemSeparator(StringBuilder sb)
        {
            sb.AppendLine(new string(ITEM_SEPARATOR_CHAR, ITEM_SEPARATOR_WIDTH));
        }

        public void AppendInfoLine(StringBuilder sb, string label, string value)
        {
            sb.AppendLine($"{label}: {value}");
        }

        public void AppendCombinedInfoLine(StringBuilder sb, params (string Label, string Value)[] items)
        {
            sb.AppendLine(string.Join(PIPE_SEPARATOR, items.Select(i => $"{i.Label}: {i.Value}")));
        }

        private void AppendCenteredText(StringBuilder sb, string text)
        {
            sb.AppendLine(text.PadLeft((LINE_WIDTH + text.Length) / 2));
        }

        public void AppendDeviceGroup(StringBuilder sb, List<(string Label, string Value)[]> devices)
        {
            for (int i = 0; i < devices.Count; i++)
            {
                foreach (var info in devices[i])
                {
                    AppendInfoLine(sb, info.Label, info.Value);
                }

                if (i < devices.Count - 1)
                {
                    AppendItemSeparator(sb);
                }
            }
        }
    }
}