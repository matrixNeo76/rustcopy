//! Milestone 3 (`tidy-sniffing-river.md`): hands a drop off to the desktop console instead of
//! spawning `robocopy_ingest.exe` directly. Milestone 2's direct-CLI spawn worked but left the
//! operator with zero visible feedback -- no window, no console, nothing to look at while a real
//! copy of several hundred MB ran silently in the background (found live, 10 Set 2026, from the
//! user's own question: "how is anyone supposed to know this is happening?"). The console already
//! has a working Esegui tab with a progress bar, a current-file indicator and a Cancel button
//! (F49) -- this reuses it wholesale via `--auto-config` (`rustcopy-gui/src/main.rs`) instead of
//! building a second, competing progress UI inside a Shell extension DLL, which would also mean
//! blocking or polling from inside `explorer.exe`, exactly what this design avoids everywhere
//! else.

use std::path::{Path, PathBuf};

/// Real destination for one dropped folder: `<drop target>\<basename of the dragged folder>`.
/// Replicates ordinary Explorer-copy semantics ("drop `Photos` onto `Backup` -> `Backup\Photos`")
/// rather than robocopy's own `/E` semantics (merge `source`'s *contents* into `dest`) -- which is
/// why this computation happens here, before `runner::write_shell_drop_config` is ever called,
/// not inside the core.
pub fn per_item_destination(drop_target: &Path, dragged_item: &Path) -> Option<PathBuf> {
    let name = dragged_item.file_name()?;
    Some(drop_target.join(name))
}

/// Launches the console on the drop, fire-and-forget: writes the throwaway config
/// (`runner::write_shell_drop_config`) and spawns `rustcopy-gui.exe --auto-config <path>`. Unlike
/// Milestone 2's direct CLI spawn, this deliberately does **not** capture stdout/stderr or set
/// `CREATE_NO_WINDOW` -- `rustcopy-gui.exe` is a windowed Tauri binary
/// (`#![windows_subsystem = "windows"]` in release), not the console-subsystem CLI, so it has no
/// console to suppress and nothing textual to capture; its own window *is* the feedback.
#[cfg(windows)]
pub fn spawn_console_run(
    gui_exe: &Path,
    items: &[(PathBuf, PathBuf)],
) -> Result<std::process::Child, robocopy_ingest::errors::IngestError> {
    let config_path = robocopy_ingest::runner::shell_drop_config_path()?;
    robocopy_ingest::runner::write_shell_drop_config(items, &config_path)?;

    let mut command = std::process::Command::new(gui_exe);
    command.args(["--auto-config", &config_path.display().to_string()]);

    command
        .spawn()
        .map_err(|error| robocopy_ingest::errors::IngestError::io(gui_exe, error))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn per_item_destination_joins_the_drop_target_with_the_dragged_folders_own_name() {
        let dest =
            per_item_destination(Path::new(r"D:\Backup"), Path::new(r"C:\Users\demo\Photos"));
        assert_eq!(dest, Some(PathBuf::from(r"D:\Backup\Photos")));
    }

    #[test]
    fn per_item_destination_is_none_for_a_path_with_no_file_name() {
        // A bare drive root has no final component -- refusing rather than guessing.
        assert_eq!(
            per_item_destination(Path::new(r"D:\Backup"), Path::new(r"C:\")),
            None
        );
    }
}
