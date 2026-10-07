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

/// Plans a drop: where each dropped folder goes (`<drop target>\<basename of the dragged folder>`)
/// and whether the drop is safe at all. Replicates ordinary Explorer-copy semantics ("drop `Photos`
/// onto `Backup` -> `Backup\Photos`") rather than robocopy's own `/E` semantics (merge `source`'s
/// *contents* into `dest`) -- which is why this happens here, before
/// `runner::write_shell_drop_config` is ever called, not inside it.
///
/// It is the console's own `runner::plan_copy` (F95), so an Explorer drop and the "Copia" tab agree
/// on what is refused: a target inside (or equal to) a dragged folder -- which would keep copying
/// the growing copy into itself, found by reading this file while building that tab --, a drive
/// root, two dragged folders with the same name. Purely lexical and allocation-only: no disk
/// access, no panic, safe to call inside `explorer.exe`.
pub fn plan_drop(
    drop_target: &Path,
    dropped: &[PathBuf],
) -> Result<Vec<(PathBuf, PathBuf)>, robocopy_ingest::errors::IngestError> {
    robocopy_ingest::runner::plan_copy(dropped, drop_target)
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
    fn plan_drop_joins_the_drop_target_with_each_dragged_folders_own_name() {
        let items = plan_drop(
            Path::new(r"D:\Backup"),
            &[PathBuf::from(r"C:\Users\demo\Photos")],
        )
        .expect("plan");
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].0, PathBuf::from(r"C:\Users\demo\Photos"));
        assert_eq!(
            items[0].1.to_string_lossy().replace('/', "\\"),
            r"D:\Backup\Photos"
        );
    }

    #[test]
    fn plan_drop_refuses_a_drive_root() {
        // A bare drive root has no folder name -- refusing rather than guessing.
        assert!(plan_drop(Path::new(r"D:\Backup"), &[PathBuf::from(r"C:\")]).is_err());
    }

    /// The case that was unguarded: dragging `C:\a` onto `C:\a\sub` (or onto itself).
    #[test]
    fn plan_drop_refuses_a_target_inside_a_dragged_folder() {
        for target in [r"C:\a\sub", r"C:\a", r"c:\A\Sub\deeper"] {
            assert!(
                plan_drop(Path::new(target), &[PathBuf::from(r"C:\a")]).is_err(),
                "{target} must be refused"
            );
        }
    }

    #[test]
    fn plan_drop_refuses_dropping_a_folder_back_onto_its_own_parent() {
        // `C:\a\Photos` dropped on `C:\a` would land on itself.
        assert!(plan_drop(Path::new(r"C:\a"), &[PathBuf::from(r"C:\a\Photos")]).is_err());
    }

    #[test]
    fn plan_drop_refuses_two_dragged_folders_with_the_same_name() {
        assert!(plan_drop(
            Path::new(r"D:\Backup"),
            &[PathBuf::from(r"C:\x\Photos"), PathBuf::from(r"E:\y\photos")],
        )
        .is_err());
    }
}
