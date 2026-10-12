//! What an elevated schedule (`--schedule-elevated`) needs from Windows: whether this process is
//! elevated, where the machine-wide data folder is, and a folder only administrators can write to.
//!
//! The point of an elevated schedule is to run a backup with administrator rights **without a prompt
//! each time** (VSS needs them, `--vss-snapshot`). That is only safe if nothing a non-elevated process
//! can edit decides what runs elevated: the schedule therefore runs a **copy** of the configuration kept
//! in a folder that only administrators can change, never the file the person edits. Both facts this
//! relies on are checked here rather than assumed: the token's elevation is read from the token (not
//! from "am I in the Administrators group"), and the machine folder comes from the Windows API (not the
//! `ProgramData` environment variable, which a non-elevated caller can set before asking for the
//! elevated process).

use std::path::{Path, PathBuf};

use crate::errors::IngestError;

/// The folder under the machine data folder that holds the copies of elevated schedules.
pub const ELEVATED_SUBDIR: &str = "rustcopy\\elevated";

/// Whether this process runs with an elevated token.
#[cfg(windows)]
pub fn is_elevated() -> bool {
    use windows_sys::Win32::Foundation::CloseHandle;
    use windows_sys::Win32::Security::{
        GetTokenInformation, TokenElevation, TOKEN_ELEVATION, TOKEN_QUERY,
    };
    use windows_sys::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};

    let mut token = std::ptr::null_mut();
    // SAFETY: plain Win32 calls with a valid out-pointer; the handle is closed on every path.
    unsafe {
        if OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) == 0 {
            return false;
        }
        let mut elevation = TOKEN_ELEVATION { TokenIsElevated: 0 };
        let mut returned = 0u32;
        let ok = GetTokenInformation(
            token,
            TokenElevation,
            std::ptr::from_mut(&mut elevation).cast(),
            std::mem::size_of::<TOKEN_ELEVATION>() as u32,
            &mut returned,
        );
        CloseHandle(token);
        ok != 0 && elevation.TokenIsElevated != 0
    }
}

#[cfg(not(windows))]
pub fn is_elevated() -> bool {
    false
}

/// The machine-wide data folder (`C:\ProgramData`), from the Windows API.
#[cfg(windows)]
pub fn program_data_dir() -> Result<PathBuf, IngestError> {
    use std::os::windows::ffi::OsStringExt;
    use windows_sys::Win32::System::Com::CoTaskMemFree;
    use windows_sys::Win32::UI::Shell::{FOLDERID_ProgramData, SHGetKnownFolderPath};

    let mut raw: *mut u16 = std::ptr::null_mut();
    // SAFETY: SHGetKnownFolderPath fills `raw` with a CoTaskMem-allocated, NUL-terminated string that
    // is read once and then freed.
    unsafe {
        let hr = SHGetKnownFolderPath(&FOLDERID_ProgramData, 0, std::ptr::null_mut(), &mut raw);
        if hr < 0 || raw.is_null() {
            return Err(IngestError::Schedule(
                "Windows non ha detto dov'è la cartella dei dati di sistema (ProgramData)."
                    .to_string(),
            ));
        }
        let mut len = 0usize;
        while *raw.add(len) != 0 {
            len += 1;
        }
        let path = PathBuf::from(std::ffi::OsString::from_wide(std::slice::from_raw_parts(
            raw, len,
        )));
        CoTaskMemFree(raw.cast());
        Ok(path)
    }
}

#[cfg(not(windows))]
pub fn program_data_dir() -> Result<PathBuf, IngestError> {
    Err(IngestError::Schedule(
        "Le pianificazioni elevate richiedono Windows.".to_string(),
    ))
}

/// The folder an elevated schedule named `name` keeps its copy and its reports in.
pub fn elevated_dir_in(program_data: &Path, name: &str) -> PathBuf {
    program_data.join(ELEVATED_SUBDIR).join(name)
}

/// The arguments of `icacls.exe` that leave `dir` writable by administrators and the system only
/// (readable by everyone, so the console can still show the reports): inheritance from the parent is
/// cut, then three grants are written with well-known SIDs (not account names, which are translated).
pub fn admin_only_acl_arguments(dir: &Path) -> Vec<String> {
    vec![
        dir.display().to_string(),
        "/inheritance:r".to_string(),
        "/grant:r".to_string(),
        "*S-1-5-32-544:(OI)(CI)F".to_string(), // BUILTIN\Administrators
        "/grant:r".to_string(),
        "*S-1-5-18:(OI)(CI)F".to_string(), // NT AUTHORITY\SYSTEM
        "/grant:r".to_string(),
        "*S-1-5-32-545:(OI)(CI)RX".to_string(), // BUILTIN\Users: read and run only
    ]
}

/// Creates `dir` and makes it administrator-write-only.
#[cfg(windows)]
pub fn create_admin_only_dir(dir: &Path) -> Result<(), IngestError> {
    std::fs::create_dir_all(dir).map_err(|e| IngestError::io(dir, e))?;
    let mut command = std::process::Command::new("icacls.exe");
    command.args(admin_only_acl_arguments(dir));
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    let output = command
        .output()
        .map_err(|source| IngestError::SpawnFailed {
            program: "icacls.exe".to_string(),
            source,
        })?;
    if output.status.success() {
        Ok(())
    } else {
        Err(IngestError::Schedule(format!(
            "non sono riuscito a limitare ai soli amministratori la cartella {}: {}",
            dir.display(),
            String::from_utf8_lossy(&output.stdout).trim()
        )))
    }
}

#[cfg(not(windows))]
pub fn create_admin_only_dir(_dir: &Path) -> Result<(), IngestError> {
    Err(IngestError::Schedule(
        "Le pianificazioni elevate richiedono Windows.".to_string(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_copy_lives_under_the_machine_folder_by_schedule_name() {
        assert_eq!(
            elevated_dir_in(Path::new("C:/ProgramData"), "notte"),
            Path::new("C:/ProgramData")
                .join("rustcopy\\elevated")
                .join("notte")
        );
    }

    #[test]
    fn the_acl_cuts_inheritance_and_grants_by_well_known_sid_only() {
        let args = admin_only_acl_arguments(Path::new("C:/ProgramData/rustcopy/elevated/x"));
        assert_eq!(args[1], "/inheritance:r");
        let grants: Vec<&String> = args.iter().filter(|a| a.starts_with('*')).collect();
        assert_eq!(grants.len(), 3);
        // Administrators and SYSTEM may write; Users only read and run.
        assert!(grants
            .iter()
            .any(|g| g.starts_with("*S-1-5-32-544") && g.ends_with(":(OI)(CI)F")));
        assert!(grants
            .iter()
            .any(|g| g.starts_with("*S-1-5-18") && g.ends_with(":(OI)(CI)F")));
        assert!(grants
            .iter()
            .any(|g| g.starts_with("*S-1-5-32-545") && g.ends_with(":(OI)(CI)RX")));
        assert!(
            !args
                .iter()
                .any(|a| a.contains("Everyone") || a.contains("S-1-1-0")),
            "nobody else is granted anything"
        );
    }

    #[cfg(windows)]
    #[test]
    fn the_machine_folder_comes_from_windows_and_exists() {
        let dir = program_data_dir().expect("known folder");
        assert!(dir.is_dir(), "{}", dir.display());
    }

    #[cfg(windows)]
    #[test]
    fn the_elevation_check_answers_without_failing() {
        // Either answer is valid on a machine; the point is that the Win32 calls are wired right.
        let _ = is_elevated();
    }
}
