//! F65: a preflight check that the destination volume has enough free space for what a run is
//! about to copy — before this, a multi-hour transfer that ran out of disk discovered the
//! problem the same way robocopy itself did: partway through, with whatever had already landed
//! left in place and the rest failed.

use std::path::{Path, PathBuf};

use crate::errors::IngestError;

/// The nearest existing ancestor of `path`, inclusive of `path` itself. `--dest` is very often
/// the directory a first-time backup is about to create, and a free-space query (here,
/// `statvfs`-style APIs generally) needs a directory that already exists to resolve which volume
/// to ask. Free space is a volume-level property, so any existing ancestor on the same volume
/// gives the identical answer `--dest` itself would once created.
fn nearest_existing_ancestor(path: &Path) -> Option<PathBuf> {
    path.ancestors().find(|p| p.exists()).map(Path::to_path_buf)
}

#[cfg(windows)]
fn free_bytes(path: &Path) -> std::io::Result<u64> {
    use std::os::windows::ffi::OsStrExt;

    use windows_sys::Win32::Storage::FileSystem::GetDiskFreeSpaceExW;

    let wide: Vec<u16> = path
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();
    let mut free_bytes_available: u64 = 0;
    // SAFETY: `wide` is a valid, NUL-terminated UTF-16 string kept alive for the duration of the
    // call. The out-parameter is a valid `*mut u64` pointing at a local whose lifetime covers the
    // call; the other two out-parameters are null, which this API accepts and simply skips.
    // `GetDiskFreeSpaceExW` does not retain any of the pointers past return.
    let ok = unsafe {
        GetDiskFreeSpaceExW(
            wide.as_ptr(),
            &mut free_bytes_available,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
        )
    };
    if ok == 0 {
        return Err(std::io::Error::last_os_error());
    }
    Ok(free_bytes_available)
}

#[cfg(not(windows))]
fn free_bytes(_path: &Path) -> std::io::Result<u64> {
    Err(std::io::Error::new(
        std::io::ErrorKind::Unsupported,
        "free-space check is only implemented on Windows",
    ))
}

/// `Err(IngestError::InsufficientDiskSpace)` when `dest` (or its nearest existing ancestor) has
/// less free space than `needed_bytes` plus `safety_margin_percent`'s worth of slack —
/// `Ok(())` otherwise, including whenever free space could not be determined at all (an unusual
/// network share, a permissions gap, no existing ancestor at all). Erring toward letting the run
/// proceed rather than blocking a backup over a check that itself failed, the same non-fatal
/// treatment `schedule::referencing_config` already gives a `schtasks.exe` query it cannot make.
pub fn ensure_enough_free_space(
    dest: &Path,
    needed_bytes: u64,
    safety_margin_percent: u32,
) -> Result<(), IngestError> {
    let Some(existing) = nearest_existing_ancestor(dest) else {
        return Ok(());
    };
    let Ok(available) = free_bytes(&existing) else {
        return Ok(());
    };
    let required = required_bytes(needed_bytes, safety_margin_percent);
    if available < required {
        return Err(IngestError::InsufficientDiskSpace {
            needed: required,
            available,
        });
    }
    Ok(())
}

/// `needed_bytes` plus `safety_margin_percent`'s worth of slack. Multiplies before dividing:
/// `needed_bytes / 100 * percent` truncates to 0 for any transfer under 100 bytes (every unit
/// test's tiny fixture tree included), which would silently zero the margin regardless of
/// `safety_margin_percent` — found writing this module's own tests, not in review.
/// `saturating_mul` keeps an enormous `needed_bytes` from wrapping instead of merely producing a
/// very large (and still correct) requirement.
fn required_bytes(needed_bytes: u64, safety_margin_percent: u32) -> u64 {
    let margin = needed_bytes.saturating_mul(u64::from(safety_margin_percent)) / 100;
    needed_bytes.saturating_add(margin)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nearest_existing_ancestor_finds_a_real_directory_above_a_missing_leaf() {
        let dir = tempfile::tempdir().expect("tempdir");
        let missing = dir.path().join("does-not-exist-yet").join("deeper");
        let found = nearest_existing_ancestor(&missing).expect("an ancestor exists");
        assert_eq!(found, dir.path());
    }

    #[test]
    fn nearest_existing_ancestor_returns_the_path_itself_when_it_already_exists() {
        let dir = tempfile::tempdir().expect("tempdir");
        let found = nearest_existing_ancestor(dir.path()).expect("exists");
        assert_eq!(found, dir.path());
    }

    #[cfg(windows)]
    #[test]
    fn free_bytes_on_a_real_directory_returns_a_plausible_value() {
        let dir = tempfile::tempdir().expect("tempdir");
        // Not asserting an exact number (it changes machine to machine and run to run) — only
        // that the call actually succeeds and returns something, not zero from an unnoticed
        // silent failure.
        let bytes = free_bytes(dir.path()).expect("query succeeds on a real directory");
        assert!(bytes > 0);
    }

    #[test]
    fn ensure_enough_free_space_passes_when_requiring_zero_bytes() {
        let dir = tempfile::tempdir().expect("tempdir");
        assert!(ensure_enough_free_space(dir.path(), 0, 5).is_ok());
    }

    #[cfg(windows)]
    #[test]
    fn ensure_enough_free_space_rejects_an_absurdly_large_requirement() {
        let dir = tempfile::tempdir().expect("tempdir");
        let error = ensure_enough_free_space(dir.path(), u64::MAX / 2, 5)
            .expect_err("no real disk has this much free space");
        assert!(matches!(error, IngestError::InsufficientDiskSpace { .. }));
    }

    #[cfg(windows)]
    #[test]
    fn the_safety_margin_increases_the_effective_requirement() {
        let dir = tempfile::tempdir().expect("tempdir");
        let available = free_bytes(dir.path()).expect("query succeeds");
        // Exactly the available space passes with no margin, but a large margin on top of that
        // same figure must push the requirement past what is actually free.
        assert!(ensure_enough_free_space(dir.path(), available, 0).is_ok());
        assert!(ensure_enough_free_space(dir.path(), available, 50).is_err());
    }

    /// Regression test: `needed_bytes / 100 * margin` (division before multiplication) truncates
    /// to zero for any `needed_bytes` under 100 — silently disabling the margin for exactly the
    /// small transfers a real backup or every unit test's own fixture tree tends to have. Calls
    /// the real function rather than re-deriving the formula, so a regression here would actually
    /// fail this test instead of just restating the fix.
    #[test]
    fn the_safety_margin_is_not_truncated_to_zero_for_a_small_requirement() {
        assert_eq!(
            required_bytes(10, 50),
            15,
            "50% of 10 bytes must add 5, not 0"
        );
        assert_eq!(required_bytes(10, 0), 10);
    }

    #[test]
    fn required_bytes_does_not_overflow_on_an_enormous_input() {
        assert_eq!(required_bytes(u64::MAX, 0), u64::MAX);
        assert!(required_bytes(u64::MAX, 50) >= u64::MAX / 2);
    }
}
