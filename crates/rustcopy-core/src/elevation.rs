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

/// The security descriptor every folder this module creates is born with, written once at creation
/// (`CreateDirectoryW`) and never applied afterwards to something that already existed: `P` cuts
/// inheritance from the parent, Administrators and SYSTEM get full control, Users only read and run (the
/// console shows the reports). Well-known SIDs, not account names, which are translated.
pub const ADMIN_ONLY_SDDL: &str = "D:P(A;OICI;FA;;;BA)(A;OICI;FA;;;SY)(A;OICI;GRGX;;;BU)";

/// Who may own a folder this module is willing to trust: Administrators, SYSTEM and TrustedInstaller.
/// A folder owned by anyone else was made by a limited user (or by something running as one), and an
/// owner can rewrite the ACL whatever it says.
const TRUSTED_OWNERS: [&str; 3] = [
    "S-1-5-32-544",
    "S-1-5-18",
    "S-1-5-80-956008885-3418522649-1831038044-1853292631-2271478464",
];

/// Whether `sid` (in its `S-1-...` string form) is one of the owners [`TRUSTED_OWNERS`] lists.
pub fn owner_is_trusted(sid: &str) -> bool {
    TRUSTED_OWNERS.iter().any(|t| t.eq_ignore_ascii_case(sid))
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
fn wide(path: &Path) -> Vec<u16> {
    use std::os::windows::ffi::OsStrExt;
    path.as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect()
}

/// Outcome of trying to create a folder.
#[cfg(windows)]
#[derive(Debug)]
enum Created {
    Fresh,
    AlreadyThere,
}

/// Creates `dir` **already carrying** [`ADMIN_ONLY_SDDL`]. There is no instant at which it exists with
/// any other permissions, and if something is already there the call reports it instead of adopting it.
#[cfg(windows)]
fn create_admin_only(dir: &Path) -> Result<Created, IngestError> {
    use windows_sys::Win32::Foundation::{LocalFree, ERROR_ALREADY_EXISTS};
    use windows_sys::Win32::Security::Authorization::{
        ConvertStringSecurityDescriptorToSecurityDescriptorW, SDDL_REVISION_1,
    };
    use windows_sys::Win32::Security::SECURITY_ATTRIBUTES;
    use windows_sys::Win32::Storage::FileSystem::CreateDirectoryW;

    let sddl: Vec<u16> = ADMIN_ONLY_SDDL.encode_utf16().chain(Some(0)).collect();
    let path = wide(dir);
    let mut descriptor = std::ptr::null_mut();
    // SAFETY: `sddl` and `path` are NUL-terminated and outlive the calls; the descriptor Windows
    // allocates is freed with LocalFree right after it is used.
    unsafe {
        if ConvertStringSecurityDescriptorToSecurityDescriptorW(
            sddl.as_ptr(),
            SDDL_REVISION_1,
            &mut descriptor,
            std::ptr::null_mut(),
        ) == 0
        {
            return Err(IngestError::io(dir, std::io::Error::last_os_error()));
        }
        let attributes = SECURITY_ATTRIBUTES {
            nLength: std::mem::size_of::<SECURITY_ATTRIBUTES>() as u32,
            lpSecurityDescriptor: descriptor,
            bInheritHandle: 0,
        };
        let ok = CreateDirectoryW(path.as_ptr(), &attributes);
        let error = std::io::Error::last_os_error();
        LocalFree(descriptor);
        if ok != 0 {
            Ok(Created::Fresh)
        } else if error.raw_os_error() == Some(ERROR_ALREADY_EXISTS as i32) {
            Ok(Created::AlreadyThere)
        } else {
            Err(IngestError::io(dir, error))
        }
    }
}

/// The owner of `path` in `S-1-...` string form, read from its security descriptor.
#[cfg(windows)]
fn owner_of(path: &Path) -> Result<String, IngestError> {
    use windows_sys::Win32::Foundation::LocalFree;
    use windows_sys::Win32::Security::Authorization::{
        ConvertSidToStringSidW, GetNamedSecurityInfoW, SE_FILE_OBJECT,
    };
    use windows_sys::Win32::Security::OWNER_SECURITY_INFORMATION;

    let wpath = wide(path);
    let mut owner = std::ptr::null_mut();
    let mut descriptor = std::ptr::null_mut();
    // SAFETY: out-pointers are valid; `owner` points into `descriptor`, which is freed only after the
    // SID string has been copied out.
    unsafe {
        let status = GetNamedSecurityInfoW(
            wpath.as_ptr(),
            SE_FILE_OBJECT,
            OWNER_SECURITY_INFORMATION,
            &mut owner,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            &mut descriptor,
        );
        if status != 0 {
            return Err(IngestError::io(
                path,
                std::io::Error::from_raw_os_error(status as i32),
            ));
        }
        let mut text: *mut u16 = std::ptr::null_mut();
        let converted = ConvertSidToStringSidW(owner, &mut text);
        let result = if converted == 0 || text.is_null() {
            Err(IngestError::io(path, std::io::Error::last_os_error()))
        } else {
            let mut len = 0usize;
            while *text.add(len) != 0 {
                len += 1;
            }
            let sid = String::from_utf16_lossy(std::slice::from_raw_parts(text, len));
            LocalFree(text.cast());
            Ok(sid)
        };
        LocalFree(descriptor);
        result
    }
}

/// Refuses `path` unless it is a real folder (not a link) owned by someone this module trusts.
#[cfg(windows)]
fn require_trusted_folder(path: &Path) -> Result<(), IngestError> {
    refuse_reparse(path)?;
    let owner = owner_of(path)?;
    if owner_is_trusted(&owner) {
        Ok(())
    } else {
        Err(IngestError::Schedule(format!(
            "la cartella {} esiste già ed è di un altro utente ({owner}), non di un amministratore: non la uso con diritti da amministratore. Cancellala da un prompt dei comandi elevato e riprova.",
            path.display()
        )))
    }
}

/// Makes `leaf` (under `root`, the machine data folder) a **fresh** folder only administrators can
/// change, and checks that nothing a limited user could have prepared is in the way.
///
/// `C:\ProgramData` lets any user create folders in it, so `rustcopy`, `rustcopy\elevated` and the
/// schedule's own folder may all exist **before** this runs, made by someone else, who may also still
/// hold an open handle to it — a handle survives any ACL change made afterwards, so taking such a folder
/// over is not an option. Instead: a missing level is created already carrying [`ADMIN_ONLY_SDDL`]; an
/// existing one must be a real folder (no junction or link) **owned by an administrator, SYSTEM or
/// TrustedInstaller**, else the install stops and says what to delete; the leaf is rebuilt on every
/// install (see `rebuild_leaf`) and every level's owner is checked once more at the end.
#[cfg(windows)]
pub fn prepare_protected_dir(root: &Path, leaf: &Path) -> Result<(), IngestError> {
    let relative = leaf
        .strip_prefix(root)
        .map_err(|_| IngestError::Schedule("cartella protetta fuori da ProgramData".to_string()))?;
    let parts: Vec<_> = relative.components().collect();
    let mut current = root.to_path_buf();
    for (index, part) in parts.iter().enumerate() {
        current.push(part);
        let is_leaf = index + 1 == parts.len();
        refuse_reparse(&current)?;
        if matches!(create_admin_only(&current)?, Created::AlreadyThere) {
            // Something is already there: it is used only if an administrator made it.
            require_trusted_folder(&current)?;
            if is_leaf {
                rebuild_leaf(&current)?;
            }
        }
        // Whatever ended up here, new or old, is checked: the owner is what the design relies on.
        require_trusted_folder(&current)?;
    }
    Ok(())
}

/// Empties an administrator-owned schedule folder of our own files, removes it and creates it again, so
/// the folder an install writes into is always one made a moment ago with [`ADMIN_ONLY_SDDL`].
/// `remove_dir` refuses a folder that still holds anything else, which is how a stranger is detected.
#[cfg(windows)]
fn rebuild_leaf(leaf: &Path) -> Result<(), IngestError> {
    for known in KNOWN_FILES {
        let file = leaf.join(known);
        refuse_reparse(&file)?;
        match std::fs::remove_file(&file) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(IngestError::io(&file, e)),
        }
    }
    std::fs::remove_dir(leaf).map_err(|e| {
        IngestError::Schedule(format!(
            "la cartella {} contiene cose che non sono di rustcopy ({e}): non la uso con diritti da amministratore",
            leaf.display()
        ))
    })?;
    match create_admin_only(leaf)? {
        Created::Fresh => Ok(()),
        Created::AlreadyThere => Err(IngestError::Schedule(format!(
            "qualcuno ha ricreato {} mentre la preparavo: non la uso con diritti da amministratore",
            leaf.display()
        ))),
    }
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
    fn the_descriptor_is_protected_and_grants_by_well_known_sid_only() {
        assert!(ADMIN_ONLY_SDDL.starts_with("D:P"), "inheritance is cut");
        let grants: Vec<&str> = ADMIN_ONLY_SDDL.split('(').skip(1).collect();
        assert_eq!(grants.len(), 3);
        // Administrators and SYSTEM may write; Users only read and run.
        assert!(grants
            .iter()
            .any(|g| g.contains(";FA;") && g.ends_with(";BA)")));
        assert!(grants
            .iter()
            .any(|g| g.contains(";FA;") && g.ends_with(";SY)")));
        assert!(grants
            .iter()
            .any(|g| g.contains(";GRGX;") && g.ends_with(";BU)")));
        assert!(
            !ADMIN_ONLY_SDDL.contains(";WD)") && !ADMIN_ONLY_SDDL.contains(";AU)"),
            "nobody else is granted anything"
        );
    }

    #[test]
    fn only_administrators_system_and_trustedinstaller_own_a_trusted_folder() {
        assert!(owner_is_trusted("S-1-5-32-544"));
        assert!(owner_is_trusted("S-1-5-18"));
        assert!(owner_is_trusted(
            "S-1-5-80-956008885-3418522649-1831038044-1853292631-2271478464"
        ));
        assert!(!owner_is_trusted("S-1-5-32-545"), "Users");
        assert!(!owner_is_trusted("S-1-5-21-1-2-3-1001"), "a plain account");
        assert!(!owner_is_trusted("S-1-1-0"), "Everyone");
        assert!(!owner_is_trusted(""));
    }

    #[test]
    fn only_junctions_and_links_count_as_reparse_points() {
        assert!(is_reparse(0x400));
        assert!(is_reparse(0x400 | 0x10), "a directory junction");
        assert!(!is_reparse(0x10), "a plain directory");
        assert!(!is_reparse(0x20), "a plain file");
    }

    /// Windows API checks against real folders: the Windows folder is an administrator's; a folder made
    /// by `create_admin_only` is born once and never adopted on a second call.
    #[cfg(windows)]
    #[test]
    fn the_windows_folder_is_trusted() {
        let windows = std::env::var_os("SystemRoot").expect("SystemRoot");
        let owner = owner_of(Path::new(&windows)).expect("owner");
        assert!(owner_is_trusted(&owner), "{owner}");
        assert!(require_trusted_folder(Path::new(&windows)).is_ok());
    }

    #[cfg(windows)]
    #[test]
    fn creation_reports_an_existing_folder_instead_of_adopting_it() {
        let base = tempfile::tempdir().expect("tempdir");
        let dir = base.path().join("x");
        assert!(matches!(create_admin_only(&dir), Ok(Created::Fresh)));
        assert!(dir.is_dir());
        assert!(matches!(create_admin_only(&dir), Ok(Created::AlreadyThere)));
    }

    /// The squatting attack: the folder a limited user made first is refused, because its owner is that
    /// user (when this test runs elevated the owner is Administrators and there is nothing to refuse).
    #[cfg(windows)]
    #[test]
    fn a_folder_somebody_else_made_first_is_refused() {
        if is_elevated() {
            return;
        }
        let root = tempfile::tempdir().expect("tempdir");
        let leaf = root.path().join("rustcopy").join("elevated").join("job");
        std::fs::create_dir_all(&leaf).expect("squatter");
        assert!(prepare_protected_dir(root.path(), &leaf).is_err());
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
