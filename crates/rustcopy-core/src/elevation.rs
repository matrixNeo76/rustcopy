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
/// (readable by everyone, so the console can still show the reports): the **owner** becomes
/// Administrators (an owner can always rewrite the ACL, so a folder a limited user created before us would
/// stay theirs otherwise), inheritance from the parent is cut, then three grants are written with
/// well-known SIDs (not account names, which are translated).
pub fn admin_only_acl_arguments(dir: &Path) -> Vec<String> {
    vec![
        dir.display().to_string(),
        "/setowner".to_string(),
        "*S-1-5-32-544".to_string(),
        "/inheritance:r".to_string(),
        "/grant:r".to_string(),
        "*S-1-5-32-544:(OI)(CI)F".to_string(), // BUILTIN\Administrators
        "/grant:r".to_string(),
        "*S-1-5-18:(OI)(CI)F".to_string(), // NT AUTHORITY\SYSTEM
        "/grant:r".to_string(),
        "*S-1-5-32-545:(OI)(CI)RX".to_string(), // BUILTIN\Users: read and run only
    ]
}

/// The files an elevated schedule keeps in its folder, and nothing else.
pub const KNOWN_FILES: [&str; 5] = [
    "config.toml",
    "task.xml",
    "robocopy_ingest.exe",
    "robocopy_ingest_report.json",
    "robocopy_ingest.log",
];

/// `FILE_ATTRIBUTE_REPARSE_POINT`: a junction or a symbolic link.
const REPARSE_POINT: u32 = 0x400;

/// Whether file attributes say the entry is a junction or symbolic link. A folder we are about to trust
/// must not be one: the elevated process would follow it to wherever a limited user pointed it.
pub fn is_reparse(attributes: u32) -> bool {
    attributes & REPARSE_POINT != 0
}

/// The names in a folder that are not ours. A limited user could have put something there before the
/// first install (a hard link, a junction with the name of a file we are about to write); nothing but our
/// own files is accepted, so an unknown entry is refused rather than guessed about.
pub fn unexpected_entries(names: &[String]) -> Vec<String> {
    names
        .iter()
        .filter(|name| {
            !KNOWN_FILES
                .iter()
                .any(|known| known.eq_ignore_ascii_case(name))
        })
        .cloned()
        .collect()
}

/// The absolute path of a program in the system folder, from the Windows API. A bare `icacls.exe` is
/// looked up in the current directory and on `PATH` first, both of which a limited process can influence
/// before asking for an elevated one.
#[cfg(windows)]
pub fn system_program(name: &str) -> PathBuf {
    use std::os::windows::ffi::OsStringExt;
    use windows_sys::Win32::System::SystemInformation::GetSystemDirectoryW;

    let mut buffer = [0u16; 260];
    // SAFETY: the buffer is valid for its length; the call reports how many units it wrote.
    let written = unsafe { GetSystemDirectoryW(buffer.as_mut_ptr(), buffer.len() as u32) } as usize;
    if written == 0 || written >= buffer.len() {
        return PathBuf::from(name);
    }
    PathBuf::from(std::ffi::OsString::from_wide(&buffer[..written])).join(name)
}

#[cfg(not(windows))]
pub fn system_program(name: &str) -> PathBuf {
    PathBuf::from(name)
}

#[cfg(windows)]
fn refuse_reparse(path: &Path) -> Result<(), IngestError> {
    use std::os::windows::fs::MetadataExt;
    match std::fs::symlink_metadata(path) {
        Ok(meta) if is_reparse(meta.file_attributes()) => Err(IngestError::Schedule(format!(
            "{} è un collegamento (giunzione o link simbolico), non una cartella vera: non lo seguo con diritti da amministratore",
            path.display()
        ))),
        _ => Ok(()),
    }
}

#[cfg(windows)]
fn restrict_to_administrators(dir: &Path) -> Result<(), IngestError> {
    let mut command = std::process::Command::new(system_program("icacls.exe"));
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

/// Makes `leaf` (under `root`, the machine data folder) a folder only administrators can change, and
/// checks that nothing a limited user could have prepared is in the way.
///
/// `C:\ProgramData` lets any user create folders in it, so `rustcopy`, `rustcopy\elevated` and the
/// schedule's own folder may all exist **before** this runs, made by someone else. Each level is
/// therefore created if missing, refused if it is a junction or link, taken over (owner set to
/// Administrators, ACL rewritten) and only then trusted; and once the leaf is locked, whatever is in it
/// must be one of our own file names or the install stops.
#[cfg(windows)]
pub fn prepare_protected_dir(root: &Path, leaf: &Path) -> Result<(), IngestError> {
    let relative = leaf
        .strip_prefix(root)
        .map_err(|_| IngestError::Schedule("cartella protetta fuori da ProgramData".to_string()))?;
    let mut current = root.to_path_buf();
    for part in relative.components() {
        current.push(part);
        refuse_reparse(&current)?;
        if !current.exists() {
            std::fs::create_dir(&current).map_err(|e| IngestError::io(&current, e))?;
        }
        restrict_to_administrators(&current)?;
        // Checked again after the lock: nothing swapped it for a link while it was being locked.
        refuse_reparse(&current)?;
    }
    let names: Vec<String> = std::fs::read_dir(leaf)
        .map_err(|e| IngestError::io(leaf, e))?
        .filter_map(Result::ok)
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .collect();
    let strangers = unexpected_entries(&names);
    if !strangers.is_empty() {
        return Err(IngestError::Schedule(format!(
            "la cartella {} contiene cose che non sono di rustcopy ({}): non la uso con diritti da amministratore",
            leaf.display(),
            strangers.join(", ")
        )));
    }
    for known in KNOWN_FILES {
        let file = leaf.join(known);
        refuse_reparse(&file)?;
        // Our own leftovers from an earlier install are replaced, never reused.
        match std::fs::remove_file(&file) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(IngestError::io(&file, e)),
        }
    }
    Ok(())
}

#[cfg(not(windows))]
pub fn prepare_protected_dir(_root: &Path, _leaf: &Path) -> Result<(), IngestError> {
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
        assert!(args.contains(&"/inheritance:r".to_string()));
        let at = args
            .iter()
            .position(|a| a == "/setowner")
            .expect("owner is set");
        assert_eq!(
            args[at + 1],
            "*S-1-5-32-544",
            "the owner becomes Administrators"
        );
        let grants: Vec<&String> = args.iter().filter(|a| a.contains(":(OI)(CI)")).collect();
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

    #[test]
    fn only_junctions_and_links_count_as_reparse_points() {
        assert!(is_reparse(0x400));
        assert!(is_reparse(0x400 | 0x10), "a directory junction");
        assert!(!is_reparse(0x10), "a plain directory");
        assert!(!is_reparse(0x20), "a plain file");
    }

    #[test]
    fn a_folder_with_anything_but_our_files_is_refused() {
        let ours: Vec<String> = ["config.toml", "ROBOCOPY_INGEST.EXE", "task.xml"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        assert!(unexpected_entries(&ours).is_empty(), "case does not matter");
        let planted: Vec<String> = ["config.toml", "payload.dll", "sub"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        assert_eq!(unexpected_entries(&planted), vec!["payload.dll", "sub"]);
    }

    /// A junction (no privilege needed to make one) is refused where a plain folder is accepted: the
    /// elevated process must not follow a link a limited user pointed somewhere else.
    #[cfg(windows)]
    #[test]
    fn a_junction_is_refused_and_a_plain_folder_is_not() {
        let base = tempfile::tempdir().expect("tempdir");
        let target = base.path().join("target");
        let plain = base.path().join("plain");
        let link = base.path().join("link");
        std::fs::create_dir(&target).expect("target");
        std::fs::create_dir(&plain).expect("plain");
        let made = std::process::Command::new("cmd")
            .args(["/C", "mklink", "/J"])
            .arg(&link)
            .arg(&target)
            .output()
            .expect("cmd");
        assert!(
            made.status.success(),
            "{}",
            String::from_utf8_lossy(&made.stdout)
        );
        assert!(
            refuse_reparse(&link).is_err(),
            "the junction must be refused"
        );
        assert!(refuse_reparse(&plain).is_ok());
        assert!(
            refuse_reparse(&base.path().join("missing")).is_ok(),
            "nothing there yet is fine"
        );
    }

    #[cfg(windows)]
    #[test]
    fn system_programs_come_from_the_system_folder_not_from_the_path() {
        let icacls = system_program("icacls.exe");
        assert!(icacls.is_absolute(), "{}", icacls.display());
        assert!(icacls.is_file(), "{}", icacls.display());
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
