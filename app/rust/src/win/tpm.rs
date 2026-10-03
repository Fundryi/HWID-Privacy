//! Read-only TPM PowerShell sources, pending the two-machine native EK parity gate.

use super::{Error, Result, process};
use std::time::Duration;

/// A PowerShell result retaining nonterminating errors alongside usable partial output.
pub struct PowerShellOutput {
    /// The legacy formatted output, without whitespace or identifier normalization.
    pub text: String,
    /// A child-process error, also retained when stdout contains usable data.
    pub failure: Option<Error>,
}

/// Reads the legacy Get-Tpm status with a 15-second process deadline.
pub fn powershell_status() -> Result<PowerShellOutput> {
    run("Get-Tpm", "Get-Tpm")
}

/// Reads the legacy endorsement-key formatting with a 15-second process deadline.
pub fn powershell_ek() -> Result<PowerShellOutput> {
    // C# parity: Hardware/TpmInfo.cs:174. Do not substitute structured/native fields
    // until every field and certificate order passes WP-05's two-TPM parity gate.
    run(
        "Get-TpmEndorsementKeyInfo",
        "Get-TpmEndorsementKeyInfo -Hash 'Sha256' | Format-List",
    )
}

fn run(op: &'static str, command: &str) -> Result<PowerShellOutput> {
    let directory = process::system32("powershell.exe");
    if !directory.is_absolute() {
        return Err(Error::msg(
            op,
            "the System32 directory could not be resolved",
        ));
    }
    let parent = directory
        .parent()
        .ok_or_else(|| Error::msg(op, "the System32 directory has no parent"))?;
    let exe = parent.join(r"WindowsPowerShell\v1.0\powershell.exe");
    // C# parity: Hardware/TpmInfo.cs:282 runs `-Command` with the user profile.
    // `-NoProfile` is a deliberate improvement: a broken or slow profile must not
    // add stderr noise or delay; cmdlet and Format-List output stay the same.
    // The shared runner owns the process/job/pipes and kills the tree on timeout.
    let output = process::run(
        &exe,
        &["-NoProfile", "-NonInteractive", "-Command", command],
        Duration::from_secs(15),
        &process::Cancel::new(),
    )
    .map_err(|error| Error::msg(op, error))?;
    let failure = if output.code != 0 || !output.stderr.trim().is_empty() {
        Some(Error {
            op,
            code: output.code as u32,
            detail: output.stderr.trim().to_owned(),
        })
    } else {
        None
    };
    // C# parity: Hardware/TpmInfo.cs:175-178,218-222. C# never reads stderr, so a
    // cmdlet error with empty stdout keeps the provider's fixed text; the error
    // goes to diagnostics. Only start failures and timeouts above are errors.
    Ok(PowerShellOutput {
        text: output.stdout,
        failure,
    })
}
