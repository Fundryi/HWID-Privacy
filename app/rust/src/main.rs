#![windows_subsystem = "windows"]

use hwidchecker::{clean, hw, report, ui, win};

use std::{
    ffi::{OsStr, OsString},
    path::{Path, PathBuf},
    process::ExitCode,
};
use windows::Win32::Foundation::HWND;

enum Mode {
    Ui,
    Dump(PathBuf),
    Time(PathBuf),
    RawSmbios(PathBuf),
    Ghosts(PathBuf),
    Logs(PathBuf),
}
struct Options {
    mode: Mode,
    only: Option<String>,
}

fn main() -> ExitCode {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    // Retain the destination even if parsing or initialization fails.
    let output_path = args
        .windows(2)
        .find(|pair| is_output_switch(&pair[0]))
        .map(|pair| Path::new(&pair[1]));
    let switch_mode = args.iter().any(|arg| is_output_switch(arg));
    std::panic::set_hook(Box::new(|info| {
        if win::is_guarded() {
            return;
        }
        let message = info
            .payload()
            .downcast_ref::<&str>()
            .copied()
            .or_else(|| info.payload().downcast_ref::<String>().map(String::as_str))
            .unwrap_or("Unknown panic");
        ui::window::show_error(
            HWND::default(),
            &format!("Unexpected error: {message}"),
            "HWID Checker",
        );
    }));
    let action = || {
        let _com = win::initialize_com().map_err(|error| error.to_string())?;
        if !win::security::is_admin() {
            return Err("HWIDChecker requires administrator privileges.".to_owned());
        }
        hwidchecker::update::cleanup_old_executables();
        parse_options(args.iter().cloned()).and_then(run)
    };
    let result = if switch_mode {
        win::catch_panic(action).and_then(|result| result)
    } else {
        action()
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            if switch_mode {
                win::record(win::Error::msg("switch mode", error.clone()));
                if let Some(path) = output_path {
                    let mut error_path = path.as_os_str().to_os_string();
                    error_path.push(".error.txt");
                    if let Err(write_error) = write(Path::new(&error_path), format!("{error}\r\n"))
                    {
                        win::record(win::Error::msg("error file", write_error));
                    }
                }
            } else {
                let title = if error == "HWIDChecker requires administrator privileges." {
                    "Elevation Required"
                } else {
                    "HWID Checker"
                };
                ui::window::show_error(HWND::default(), &error, title);
            }
            ExitCode::FAILURE
        }
    }
}

fn is_output_switch(arg: &OsStr) -> bool {
    matches!(
        arg.to_str(),
        Some("--dump" | "--time" | "--dump-raw-smbios" | "--ghosts" | "--logs")
    )
}

fn parse_options(args: impl Iterator<Item = OsString>) -> Result<Options, String> {
    let mut args = args.peekable();
    let mut mode = None;
    let mut only = None;
    while let Some(arg) = args.next() {
        let flag = arg.to_str().ok_or("Switch must be valid Unicode")?;
        if flag == "--only" {
            if only.is_some() {
                return Err("--only specified more than once".to_owned());
            }
            only = Some(
                args.next()
                    .ok_or("--only requires a section title")?
                    .into_string()
                    .map_err(|_| "Section title must be valid Unicode")?,
            );
            continue;
        }
        if !is_output_switch(&arg) {
            return Err(format!("Unknown switch: {flag}"));
        }
        if mode.is_some() {
            return Err("Use one output switch at a time".to_owned());
        }
        let path = PathBuf::from(
            args.next()
                .ok_or_else(|| format!("{flag} requires an output file"))?,
        );
        mode = Some(match flag {
            "--dump" => Mode::Dump(path),
            "--time" => Mode::Time(path),
            "--dump-raw-smbios" => Mode::RawSmbios(path),
            "--ghosts" => Mode::Ghosts(path),
            _ => Mode::Logs(path),
        });
    }
    let mode = mode.unwrap_or(Mode::Ui);
    if let Some(title) = &only {
        if !matches!(mode, Mode::Dump(_)) {
            return Err("--only requires --dump".to_owned());
        }
        if !hw::PROVIDERS
            .iter()
            .any(|p| report::eq_ignore_case(title, p.title))
        {
            return Err(format!("No provider found for section: {title}"));
        }
    }
    Ok(Options { mode, only })
}

fn write(path: &Path, contents: impl AsRef<[u8]>) -> Result<(), String> {
    std::fs::write(path, contents)
        .map_err(|error| format!("Write {} failed: {error}", path.display()))
}

fn run(options: Options) -> Result<(), String> {
    match options.mode {
        Mode::Ui => {
            ui::run();
            Ok(())
        }
        Mode::Dump(path) => {
            let sections = hw::collect_all(options.only.as_deref(), &|_, _| {});
            write(&path, hw::full_report(&sections))?;
            let mut diagnostics = String::new();
            for section in &sections {
                diagnostics.push_str(&format!(
                    "{}\r\nTime: {} ms\r\nSource: {}\r\n",
                    section.title, section.elapsed_ms, section.source
                ));
                for failure in &section.failures {
                    diagnostics.push_str(&format!("Failed fallback: {failure}\r\n"));
                }
                diagnostics.push_str("\r\n");
            }
            diagnostics.push_str("[helpers]\r\n");
            for error in win::take_recorded() {
                diagnostics.push_str(&format!("{error}\r\n"));
            }
            let mut diag_path = path.into_os_string();
            diag_path.push(".diag.txt");
            write(Path::new(&diag_path), diagnostics)
        }
        Mode::Time(path) => {
            let mut tsv = "Section\tMedian ms\r\n".to_owned();
            for provider in &hw::PROVIDERS {
                let mut times = [0_u128; 5];
                for elapsed in &mut times {
                    let sections = hw::collect_all(Some(provider.title), &|_, _| {});
                    *elapsed = sections
                        .first()
                        .ok_or_else(|| format!("No timing sample for {}", provider.title))?
                        .elapsed_ms;
                }
                times.sort_unstable();
                tsv.push_str(&format!("{}\t{}\r\n", provider.title, times[2]));
            }
            write(&path, tsv)
        }
        Mode::RawSmbios(path) => {
            let raw = win::firmware::raw_table(u32::from_be_bytes(*b"RSMB"), 0)
                .map_err(|e| e.to_string())?;
            write(&path, raw)
        }
        Mode::Ghosts(path) => {
            let scan = clean::devices::scan()?;
            let mut text = String::new();
            for device in scan.devices() {
                text.push_str(&format!(
                    "{} | {} | {} | {} | {:?}\r\n",
                    device.description,
                    device.class,
                    device.hardware_id,
                    device.instance_id,
                    device.presence
                ));
            }
            write(&path, text)
        }
        Mode::Logs(path) => {
            let (standard, additional) = clean::eventlog::planned_logs();
            let mut text = String::new();
            for name in standard.iter().chain(&additional) {
                text.push_str(name);
                text.push_str("\r\n");
            }
            write(&path, text)
        }
    }
}
