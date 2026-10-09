//! "Espelli l'unità alla fine" (CATALOGO_COMPORTAMENTI_GUI.md L36): after a clean copy onto a **removable**
//! drive, the drive is locked, dismounted and ejected the way Explorer does it, so the person can pull it
//! out without a second step.
//!
//! Only a drive letter whose type Windows reports as *removable* is ever touched: never a fixed disk, a
//! network drive, an optical drive or a path that does not start with a letter. If the volume is still in
//! use the lock fails and nothing is done, and the person is told.

/// The drive letter a destination path starts with (`E:\backup` -> `'E'`), or `None` for a network path, a
/// relative path or anything else. Plain string logic, so it answers the same on every host.
pub fn drive_letter(path: &str) -> Option<char> {
    let mut chars = path.trim_start().chars();
    let letter = chars.next()?;
    let colon = chars.next()?;
    (letter.is_ascii_alphabetic() && colon == ':').then(|| letter.to_ascii_uppercase())
}

/// What an eject attempt came to.
#[derive(Debug, PartialEq, Eq)]
pub enum Outcome {
    Ejected,
    /// The volume is in use by something else: nothing was changed.
    InUse,
    /// Not a removable drive (or not a drive at all): nothing was changed.
    NotRemovable,
    /// Any other failure, with the system's own text.
    Failed(String),
}

#[cfg(windows)]
mod win {
    use super::Outcome;
    use std::os::windows::ffi::OsStrExt;

    use windows_sys::Win32::Foundation::{
        CloseHandle, GetLastError, ERROR_ACCESS_DENIED, ERROR_SHARING_VIOLATION, GENERIC_READ,
        GENERIC_WRITE, INVALID_HANDLE_VALUE,
    };
    use windows_sys::Win32::Storage::FileSystem::{
        CreateFileW, GetDriveTypeW, FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING,
    };
    use windows_sys::Win32::System::Ioctl::{
        FSCTL_DISMOUNT_VOLUME, FSCTL_LOCK_VOLUME, IOCTL_STORAGE_EJECT_MEDIA,
        IOCTL_STORAGE_MEDIA_REMOVAL, PREVENT_MEDIA_REMOVAL,
    };
    use windows_sys::Win32::System::IO::DeviceIoControl;

    /// `DRIVE_REMOVABLE` from `GetDriveTypeW`.
    const DRIVE_REMOVABLE: u32 = 2;

    fn wide(text: &str) -> Vec<u16> {
        std::ffi::OsStr::new(text)
            .encode_wide()
            .chain(std::iter::once(0))
            .collect()
    }

    pub fn is_removable(letter: char) -> bool {
        let root = wide(&format!("{letter}:\\"));
        // SAFETY: `root` is a valid NUL-terminated UTF-16 string that outlives the call.
        unsafe { GetDriveTypeW(root.as_ptr()) == DRIVE_REMOVABLE }
    }

    fn control(
        handle: *mut core::ffi::c_void,
        code: u32,
        input: *const core::ffi::c_void,
        size: u32,
    ) -> bool {
        let mut returned = 0u32;
        // SAFETY: `handle` is an open volume handle; `input` points to `size` readable bytes (or is null
        // with size 0); no output buffer is requested; `returned` is a valid local.
        unsafe {
            DeviceIoControl(
                handle,
                code,
                input,
                size,
                std::ptr::null_mut(),
                0,
                &mut returned,
                std::ptr::null_mut(),
            ) != 0
        }
    }

    pub fn eject(letter: char) -> Outcome {
        if !is_removable(letter) {
            return Outcome::NotRemovable;
        }
        let path = wide(&format!("\\\\.\\{letter}:"));
        // SAFETY: `path` is a valid NUL-terminated UTF-16 string; the handle is closed on every path below.
        let handle = unsafe {
            CreateFileW(
                path.as_ptr(),
                GENERIC_READ | GENERIC_WRITE,
                FILE_SHARE_READ | FILE_SHARE_WRITE,
                std::ptr::null(),
                OPEN_EXISTING,
                0,
                std::ptr::null_mut(),
            )
        };
        if handle == INVALID_HANDLE_VALUE {
            // SAFETY: reads the calling thread's last error.
            let code = unsafe { GetLastError() };
            return if code == ERROR_ACCESS_DENIED || code == ERROR_SHARING_VIOLATION {
                Outcome::InUse
            } else {
                Outcome::Failed(std::io::Error::from_raw_os_error(code as i32).to_string())
            };
        }
        // The lock fails while anything has a file open on the volume: that is the "in use" answer, and
        // it changes nothing.
        let outcome = if !control(handle, FSCTL_LOCK_VOLUME, std::ptr::null(), 0) {
            Outcome::InUse
        } else {
            control(handle, FSCTL_DISMOUNT_VOLUME, std::ptr::null(), 0);
            let allow = PREVENT_MEDIA_REMOVAL {
                PreventMediaRemoval: false,
            };
            control(
                handle,
                IOCTL_STORAGE_MEDIA_REMOVAL,
                std::ptr::addr_of!(allow).cast(),
                std::mem::size_of::<PREVENT_MEDIA_REMOVAL>() as u32,
            );
            if control(handle, IOCTL_STORAGE_EJECT_MEDIA, std::ptr::null(), 0) {
                Outcome::Ejected
            } else {
                // SAFETY: reads the calling thread's last error.
                let code = unsafe { GetLastError() };
                Outcome::Failed(std::io::Error::from_raw_os_error(code as i32).to_string())
            }
        };
        // SAFETY: `handle` is valid and closed exactly once.
        unsafe {
            CloseHandle(handle);
        }
        outcome
    }
}

/// Whether `letter` is a removable drive right now.
#[cfg(windows)]
pub fn is_removable(letter: char) -> bool {
    win::is_removable(letter)
}

#[cfg(not(windows))]
pub fn is_removable(_letter: char) -> bool {
    false
}

/// Whether a destination path is on a removable drive.
pub fn destination_is_removable(path: &str) -> bool {
    drive_letter(path).is_some_and(is_removable)
}

/// Ejects the drive `letter` if it is removable.
#[cfg(windows)]
pub fn eject(letter: char) -> Outcome {
    win::eject(letter)
}

#[cfg(not(windows))]
pub fn eject(_letter: char) -> Outcome {
    Outcome::NotRemovable
}

/// What the person is told about the outcome.
pub fn message(letter: char, outcome: &Outcome) -> String {
    match outcome {
        Outcome::Ejected => format!("L'unità {letter}: è stata espulsa: puoi staccarla."),
        Outcome::InUse => format!(
            "L'unità {letter}: è ancora in uso da un altro programma e non l'ho espulsa. Chiudi i file aperti ed espellila da Esplora file."
        ),
        Outcome::NotRemovable => format!("L'unità {letter}: non è un'unità rimovibile: non la espello."),
        Outcome::Failed(why) => format!("Non sono riuscito a espellere {letter}: ({why}). Espellila da Esplora file."),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_a_path_that_starts_with_a_drive_letter_names_a_drive() {
        assert_eq!(drive_letter(r"E:\backup\foto"), Some('E'));
        assert_eq!(drive_letter("e:/x"), Some('E'));
        assert_eq!(drive_letter("  F:"), Some('F'));
        assert_eq!(drive_letter(r"\\nas\share"), None);
        assert_eq!(drive_letter("backup"), None);
        assert_eq!(drive_letter(""), None);
        assert_eq!(drive_letter("1:\\x"), None);
    }

    #[test]
    fn a_network_or_relative_destination_is_never_removable() {
        assert!(!destination_is_removable(r"\\nas\share\x"));
        assert!(!destination_is_removable("relative\\path"));
        assert!(!destination_is_removable(""));
    }

    #[cfg(windows)]
    #[test]
    fn the_system_drive_is_not_removable_and_is_never_ejected() {
        // `C:` is a fixed disk on any machine that runs this test.
        assert!(!is_removable('C'));
        assert_eq!(eject('C'), Outcome::NotRemovable);
    }

    #[test]
    fn every_outcome_has_a_sentence_that_names_the_drive() {
        for outcome in [
            Outcome::Ejected,
            Outcome::InUse,
            Outcome::NotRemovable,
            Outcome::Failed("x".to_string()),
        ] {
            assert!(message('E', &outcome).contains('E'));
        }
    }
}
