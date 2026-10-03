using System.Diagnostics;
using System.Globalization;
using System.Reflection;
using System.Text;
using System.Text.Json;
using HWIDChecker.Hardware;
using HWIDChecker.Services;

namespace GoldenDump;

internal static class Program
{
    private static readonly Encoding Utf8 = new UTF8Encoding(false);
    private const BindingFlags PrivateInstance = BindingFlags.Instance | BindingFlags.NonPublic;

    [STAThread]
    private static async Task<int> Main(string[] args)
    {
        Console.OutputEncoding = Utf8;
        if (args.Length == 0 || args.Length > 3)
            return Usage();

        string mode = args[0].StartsWith("--", StringComparison.Ordinal) ? args[0] : "report";
        if ((mode == "report" && args.Length != 1) ||
            (mode == "--format" && args.Length != 2 && args.Length != 3) ||
            (mode != "report" && mode != "--format" &&
             (args.Length != 2 || mode is not ("--time" or "--ghosts" or "--logs"))))
            return Usage();

        try
        {
            // Match the legacy app's elevation gate for hardware collection.
            if (mode is "report" or "--time" or "--ghosts" && !SecurityHelper.IsAdministrator())
            {
                Console.Error.WriteLine("HWIDChecker requires administrator privileges. Run GoldenDump in an elevated PowerShell.");
                return 1;
            }

            string output = mode switch
            {
                "report" => await new HardwareInfoManager().GetAllHardwareInfo().WaitAsync(TimeSpan.FromSeconds(60)),
                "--time" => await Timings(),
                "--ghosts" => Ghosts(),
                "--logs" => await Logs(),
                "--format" => Format(args[1]),
                _ => throw new InvalidOperationException("Unknown mode.")
            };

            if (mode == "--format" && args.Length == 2)
                Console.Write(output);
            else
                await File.WriteAllTextAsync(args[mode == "report" ? 0 : args.Length - 1], output, Utf8);
            return 0;
        }
        catch (Exception ex)
        {
            if (ex is TargetInvocationException { InnerException: not null } invocation)
                ex = invocation.InnerException;
            Console.Error.WriteLine($"GoldenDump {mode} failed: {ex.Message}");
            return 1;
        }
    }

    private static int Usage()
    {
        Console.Error.WriteLine("Usage: GoldenDump <report.txt> | --time <time.tsv> | --ghosts <ghosts.txt> | --logs <logs.txt> | --format <input.json> [output.txt]");
        Console.Error.WriteLine("--format takes an array of calls: {\"method\":\"FormatHeader\",\"text\":\"fabricated text\"}. Use only fabricated inputs.");
        return 2;
    }

    private static async Task<string> Timings()
    {
        var manager = new HardwareInfoManager();
        var providers = (List<IHardwareInfo>)typeof(HardwareInfoManager)
            .GetField("hardwareInfoProviders", PrivateInstance).GetValue(manager);
        var output = new StringBuilder();
        output.AppendLine("Section\tMedianMs");
        // Each --time invocation is a fresh process. Preserve legacy provider order.
        foreach (var provider in providers)
        {
            var times = new double[5];
            for (int run = 0; run < times.Length; run++)
            {
                var timer = Stopwatch.StartNew();
                await Task.Run(() => provider.GetInformation()).WaitAsync(TimeSpan.FromSeconds(60));
                times[run] = timer.Elapsed.TotalMilliseconds;
            }
            Array.Sort(times);
            output.Append(provider.SectionTitle).Append('\t')
                .AppendLine(times[2].ToString("F3", CultureInfo.InvariantCulture));
        }
        return output.ToString();
    }

    private static string Ghosts()
    {
        var service = new DeviceCleaningService();
        try
        {
            var output = new StringBuilder();
            foreach (var device in service.ScanForGhostDevices())
            {
                // Legacy DeviceDetail.Name carries the flag; no InstanceId exists.
                output.AppendLine($"{device.Description} | {device.Class} | {device.HardwareId} |  | {device.Name}");
            }
            return output.ToString();
        }
        finally
        {
            // Scan retains its HDEVINFO for removal. Release it without removing anything.
            var handleField = typeof(DeviceCleaningService).GetField("_devicesHandle", PrivateInstance);
            var handle = (IntPtr)handleField.GetValue(service);
            if (handle != IntPtr.Zero && handle.ToInt64() != -1)
            {
                var released = (bool)typeof(DeviceCleaningService)
                    .GetMethod("SetupDiDestroyDeviceInfoList", BindingFlags.Static | BindingFlags.NonPublic)
                    .Invoke(null, new object[] { handle });
                if (!released)
                    throw new InvalidOperationException("Failed to release the ghost scan device list.");
                handleField.SetValue(service, IntPtr.Zero);
            }
        }
    }

    private static async Task<string> Logs()
    {
        var service = new EventLogCleaningService();
        // Discovery only. Never call CleanEventLogsAsync, which changes the system.
        service.OnStatusUpdate += message => Console.Error.WriteLine(message);
        var type = typeof(EventLogCleaningService);
        var standard = (string[])type.GetField("StandardEventLogs", PrivateInstance).GetValue(service);
        var logs = (List<string>)type.GetMethod("BuildUniqueLogList", BindingFlags.Static | BindingFlags.NonPublic)
            .Invoke(null, new object[] { standard, 0 });
        var known = new HashSet<string>(logs, StringComparer.OrdinalIgnoreCase);
        using var timeout = new CancellationTokenSource(TimeSpan.FromSeconds(60));
        var additionalTask = (Task<List<string>>)type.GetMethod("TryCollectAdditionalLogsAsync", PrivateInstance)
            .Invoke(service, new object[] { known, timeout.Token });
        logs.AddRange(await additionalTask.WaitAsync(timeout.Token));
        return string.Concat(logs.Select(log => log + Environment.NewLine));
    }

    private static string Format(string path)
    {
        using var document = JsonDocument.Parse(File.ReadAllText(path));
        var formatter = new TextFormattingService();
        var output = new StringBuilder();
        // JSON is an array of public TextFormattingService calls, concatenated verbatim.
        foreach (var call in document.RootElement.EnumerateArray())
        {
            string method = call.GetProperty("method").GetString();
            switch (method)
            {
                case "FormatHeader":
                    output.Append(formatter.FormatHeader(call.GetProperty("text").GetString()));
                    break;
                case "FormatSection":
                    output.Append(formatter.FormatSection(call.GetProperty("title").GetString(), call.GetProperty("content").GetString()));
                    break;
                case "AppendInfoLine":
                    formatter.AppendInfoLine(output, call.GetProperty("label").GetString(), call.GetProperty("value").GetString());
                    break;
                case "AppendCombinedInfoLine":
                    formatter.AppendCombinedInfoLine(output, Items(call.GetProperty("items")));
                    break;
                case "AppendItemSeparator":
                    formatter.AppendItemSeparator(output);
                    break;
                case "AppendDeviceGroup":
                    formatter.AppendDeviceGroup(output, call.GetProperty("devices").EnumerateArray().Select(Items).ToList());
                    break;
                default:
                    throw new ArgumentException($"Unknown TextFormattingService method: {method}");
            }
        }
        return output.ToString();
    }

    private static (string Label, string Value)[] Items(JsonElement items) => items.EnumerateArray()
        .Select(item => (item.GetProperty("label").GetString(), item.GetProperty("value").GetString())).ToArray();
}
