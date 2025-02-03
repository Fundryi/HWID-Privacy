using System;
using System.IO;
using System.Windows.Forms;
using System.Collections.Generic;
using System.Linq;

namespace HWIDChecker.Services
{
    public class FileExportService
    {
        private readonly string basePath;

        public FileExportService(string basePath)
        {
            this.basePath = basePath;
        }

        public string ExportHardwareInfo(string content)
        {
            var date = DateTime.Now.ToString("dd.MM.yyyy");
            var time = DateTime.Now.ToString("HH;mm;ss");
            var filePath = Path.Combine(basePath, $"HWID-EXPORT-{date}-{time}.txt");

            try
            {
                File.WriteAllText(filePath, content);
                return filePath;
            }
            catch (Exception)
            {
                throw;
            }
        }

        public List<string> GetExportedFiles()
        {
            return Directory.GetFiles(basePath, "HWID-EXPORT-*.txt")
                .OrderByDescending(f => f)
                .ToList();
        }

        public string ReadExportedFile(string filePath)
        {
            try
            {
                return File.ReadAllText(filePath);
            }
            catch (Exception)
            {
                throw;
            }
        }
    }
}