using System;
using System.Collections.Generic;
using System.Windows.Forms;
using System.IO;
using HWIDChecker.UI.Components;

namespace HWIDChecker.Forms
{
    public class CompareForm : Form
    {
        private readonly CompareFormLayout layout;

        private CompareForm(string leftContent)
        {
            layout = new CompareFormLayout();
            layout.InitializeLayout(this);

            var mainContainer = layout.CreateMainContainer();

            var leftPanel = layout.CreateSidePanel("Current Configuration");
            leftPanel.Controls.Add(layout.LeftText, 0, 1);
            layout.LeftText.Text = leftContent;

            var rightPanel = layout.CreateSidePanel("Comparison Configuration");
            rightPanel.Controls.Add(layout.RightText, 0, 1);

            mainContainer.Controls.Add(leftPanel, 0, 0);
            mainContainer.Controls.Add(rightPanel, 1, 0);

            Controls.Add(mainContainer);
        }

        public static CompareForm CreateCompareWithCurrent(string currentConfig, List<string> exportFiles)
        {
            if (exportFiles.Count == 0) return null;

            var form = new CompareForm(currentConfig);
            form.layout.RightText.Text = File.ReadAllText(exportFiles[0]);
            return form;
        }

        public static CompareForm CreateCompareExported(List<string> exportFiles)
        {
            if (exportFiles.Count < 2) return null;

            var form = new CompareForm(File.ReadAllText(exportFiles[0]));
            form.layout.RightText.Text = File.ReadAllText(exportFiles[1]);
            return form;
        }
    }
}