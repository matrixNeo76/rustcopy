//! Pure logic for the stub's one observable effect: appending a line to a log file. Split out
//! so the path computation is unit-testable without a real COM host (this crate's declared
//! testing limit -- see the module doc in `lib.rs` -- is the COM plumbing itself, not the plain
//! Rust functions around it).

use std::path::PathBuf;

/// `%TEMP%\rustcopy-shell\` -- under the *user's* temp directory (not a fixed system path),
/// since the DLL runs inside `explorer.exe` under the interactively logged-on user, same as any
/// other per-user Shell extension state. Shared by the Milestone 1 stub log ([`log_path`]) and
/// Milestone 2's per-spawn captured output (`spawn::output_file_for`).
pub fn log_dir() -> PathBuf {
    std::env::temp_dir().join("rustcopy-shell")
}

/// `%TEMP%\rustcopy-shell\stub.log`.
pub fn log_path() -> PathBuf {
    log_dir().join("stub.log")
}

pub fn append_line(path: &std::path::Path, line: &str) -> std::io::Result<()> {
    use std::io::Write;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)?;
    writeln!(file, "{line}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn log_path_lives_under_the_user_temp_directory() {
        let path = log_path();
        assert!(path.starts_with(std::env::temp_dir()));
        assert_eq!(path.file_name().unwrap(), "stub.log");
    }

    #[test]
    fn append_line_creates_parent_directories_and_appends() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("nested").join("stub.log");

        append_line(&path, "first").unwrap();
        append_line(&path, "second").unwrap();

        let content = std::fs::read_to_string(&path).unwrap();
        assert_eq!(content, "first\nsecond\n");
    }
}
