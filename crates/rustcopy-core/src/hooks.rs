//! F39: pre/post job commands — Cobian's "events". Runs an arbitrary, operator-supplied shell
//! command before and/or after a job (e.g. stop a database service before backing it up, restart
//! it afterwards). The command string is exactly what `--pre-command`/`--post-command` or the
//! TOML config gave — same trust boundary as `--webhook-url`: entirely operator-supplied, not
//! attacker-controlled input, so no escaping/sandboxing is attempted here.

use std::process::{Command, ExitStatus};

use crate::errors::IngestError;

/// Runs `command` through the platform shell and waits for it to finish: `cmd.exe /C` on Windows,
/// `sh -c` elsewhere (kept cross-platform so the unit tests below run on Linux/macOS too, even
/// though the rest of the crate's real transfers are Windows-only).
fn run_command(command: &str) -> std::io::Result<ExitStatus> {
    #[cfg(windows)]
    {
        Command::new("cmd").args(["/C", command]).status()
    }
    #[cfg(not(windows))]
    {
        Command::new("sh").args(["-c", command]).status()
    }
}

/// Runs the pre-job command, if any, and fails the run if it exits non-zero (or can't even be
/// spawned). A pre-command is typically "stop the database before backing it up" — proceeding
/// with the backup after that failed would silently back up a live, possibly inconsistent,
/// database, so this is a hard failure rather than a warning.
pub fn run_pre_command(command: &str) -> Result<(), IngestError> {
    let status = run_command(command).map_err(|source| IngestError::SpawnFailed {
        program: command.to_string(),
        source,
    })?;
    if !status.success() {
        return Err(IngestError::PreCommandFailed {
            command: command.to_string(),
            code: status.code(),
        });
    }
    Ok(())
}

/// Runs the post-job command, if any. Unlike the pre-command, a failure here does not fail the
/// run — the backup itself already succeeded by the time the post-command runs, so a broken
/// "restart the database" step shouldn't retroactively turn a successful backup into a failed
/// one. Returns a human-readable description of what went wrong, if anything, for the caller to
/// log and record in the report (mirrors `notify::send_webhook`'s error-reporting pattern).
pub fn run_post_command(command: &str) -> Option<String> {
    match run_command(command) {
        Ok(status) if status.success() => None,
        Ok(status) => Some(format!(
            "post-command {command:?} exited with status {}",
            status
                .code()
                .map(|c| c.to_string())
                .unwrap_or_else(|| "unknown (terminated by signal)".to_string())
        )),
        Err(error) => Some(format!(
            "post-command {command:?} could not be spawned: {error}"
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pre_command_succeeds_when_the_command_exits_zero() {
        assert!(run_pre_command("exit 0").is_ok());
    }

    #[test]
    fn pre_command_fails_when_the_command_exits_non_zero() {
        let error = run_pre_command("exit 3").expect_err("must fail");
        assert!(matches!(
            error,
            IngestError::PreCommandFailed { code: Some(3), .. }
        ));
    }

    #[test]
    fn post_command_returns_none_when_the_command_exits_zero() {
        assert!(run_post_command("exit 0").is_none());
    }

    #[test]
    fn post_command_returns_a_description_when_the_command_exits_non_zero() {
        let description = run_post_command("exit 5").expect("must describe the failure");
        assert!(description.contains('5'), "description: {description}");
    }
}
