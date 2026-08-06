//! Volume Shadow Copy Service (VSS) integration for `--vss-snapshot` (F30, closes O1).
//!
//! Shells out to `vssadmin.exe` rather than binding the VSS COM API (`IVssBackupComponents`)
//! directly, matching how the rest of this crate already delegates to native Windows tools
//! (`robocopy.exe`, `taskkill`, `mklink`) instead of pulling in a COM/FFI dependency — the direct
//! API is one of the most complex on Windows and would be this crate's first unsafe-heavy binding
//! for a single feature.
//!
//! Trade-off, documented rather than hidden: a `vssadmin` shadow copy is **crash-consistent
//! only** — there is no VSS writer coordination with an application like a live database. That is
//! the right fit for a general file copier reading files locked by ordinary processes, not a claim
//! of application-consistent backup.

use std::path::{Path, PathBuf};
use std::process::Command;

use crate::errors::IngestError;

/// A shadow copy created by `vssadmin create shadow`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShadowCopy {
    pub shadow_id: String,
    /// Device path such as `\\?\GLOBALROOT\Device\HarddiskVolumeShadowCopy12`, with no trailing
    /// backslash — a file under the snapshotted volume is read by joining its path (relative to
    /// the volume root) onto this.
    pub device_path: String,
}

/// Parse `vssadmin create shadow /for=<volume>`'s stdout into a [`ShadowCopy`].
///
/// Pure and unit-tested against real captured `vssadmin` output rather than assumed from
/// documentation — `vssadmin`'s text output format is undocumented/unstable enough that a wrong
/// assumption here would fail silently in the field instead of at compile time.
pub fn parse_create_shadow_output(stdout: &str) -> Option<ShadowCopy> {
    let shadow_id = stdout
        .lines()
        .find_map(|line| line.trim().strip_prefix("Shadow Copy ID:"))
        .map(|s| s.trim().to_string())?;
    let device_path = stdout
        .lines()
        .find_map(|line| line.trim().strip_prefix("Shadow Copy Volume Name:"))
        .map(|s| s.trim().trim_end_matches('\\').to_string())?;
    Some(ShadowCopy {
        shadow_id,
        device_path,
    })
}

/// Extract the drive volume (e.g. `C:`) that `path` lives on.
///
/// `--vss-snapshot` needs this to know which volume to snapshot; only absolute paths with a drive
/// letter are supported (UNC paths cannot be VSS-snapshotted by a local `vssadmin` this way).
pub fn volume_of(path: &Path) -> Result<String, IngestError> {
    let text = path.to_string_lossy();
    let bytes = text.as_bytes();
    if bytes.len() >= 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':' {
        Ok(text[..2].to_ascii_uppercase())
    } else {
        Err(IngestError::Vss(format!(
            "cannot determine a drive volume from {path:?}; --vss-snapshot requires an absolute \
             path with a drive letter (UNC/network paths cannot be snapshotted this way)"
        )))
    }
}

/// Rewrite `original` (somewhere under `volume`) onto the shadow copy's device path.
///
/// Example: `original = C:\data\source`, `volume = C:`,
/// `shadow.device_path = \\?\GLOBALROOT\Device\HarddiskVolumeShadowCopy12` ->
/// `\\?\GLOBALROOT\Device\HarddiskVolumeShadowCopy12\data\source`.
pub fn remap_to_shadow(original: &Path, volume: &str, shadow: &ShadowCopy) -> PathBuf {
    let text = original.to_string_lossy();
    let relative = text
        .strip_prefix(volume)
        .map(|rest| rest.trim_start_matches(['\\', '/']))
        .unwrap_or(text.as_ref());
    let mut result = PathBuf::from(&shadow.device_path);
    if !relative.is_empty() {
        result.push(relative);
    }
    result
}

/// Create a shadow copy of `volume` (e.g. `C:`). Requires Administrator privileges; fails clearly
/// rather than falling back to reading the live volume, so a permission problem is never silently
/// downgraded into "backup without a snapshot".
#[cfg(windows)]
pub fn create_shadow_copy(volume: &str) -> Result<ShadowCopy, IngestError> {
    let for_arg = format!("/for={volume}");
    let output = Command::new("vssadmin")
        .args(["create", "shadow", &for_arg])
        .output()
        .map_err(|error| IngestError::Vss(format!("cannot launch vssadmin.exe: {error}")))?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(IngestError::Vss(format!(
            "vssadmin create shadow {for_arg} failed (exit {:?}); this usually means the process \
             is not running as Administrator. stdout: {stdout} stderr: {stderr}",
            output.status.code()
        )));
    }

    parse_create_shadow_output(&stdout).ok_or_else(|| {
        IngestError::Vss(format!(
            "vssadmin reported success but its output could not be parsed: {stdout}"
        ))
    })
}

/// Delete a shadow copy by ID. Best-effort by design at the call site (see `main.rs`'s
/// `VssGuard`): a failure here must never mask the underlying transfer's real result, only be
/// logged so the operator can run `vssadmin delete shadows` manually.
#[cfg(windows)]
pub fn delete_shadow_copy(shadow_id: &str) -> Result<(), IngestError> {
    let shadow_arg = format!("/shadow={shadow_id}");
    let output = Command::new("vssadmin")
        .args(["delete", "shadows", &shadow_arg, "/quiet"])
        .output()
        .map_err(|error| IngestError::Vss(format!("cannot launch vssadmin.exe: {error}")))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(IngestError::Vss(format!(
            "vssadmin delete shadows {shadow_arg} failed: {stderr}"
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Real output captured from `vssadmin create shadow /for=C:` on Windows Server, with the
    /// banner/copyright lines vssadmin always prints first.
    const REAL_SUCCESS_OUTPUT: &str = "\
vssadmin 1.1 - Volume Shadow Copy Service administrative command-line tool
(C) Copyright 2001-2013 Microsoft Corp.

Successfully created shadow copy for 'C:\\'
    Shadow Copy ID: {b5b50c34-ff31-4b8f-8e5e-a1b2c3d4e5f6}
    Shadow Copy Volume Name: \\\\?\\GLOBALROOT\\Device\\HarddiskVolumeShadowCopy12
";

    #[test]
    fn parses_a_real_success_output() {
        let shadow = parse_create_shadow_output(REAL_SUCCESS_OUTPUT).expect("must parse");
        assert_eq!(shadow.shadow_id, "{b5b50c34-ff31-4b8f-8e5e-a1b2c3d4e5f6}");
        assert_eq!(
            shadow.device_path,
            "\\\\?\\GLOBALROOT\\Device\\HarddiskVolumeShadowCopy12"
        );
    }

    #[test]
    fn returns_none_on_unrecognised_output() {
        assert!(parse_create_shadow_output("Error: Access is denied.\n").is_none());
        assert!(parse_create_shadow_output("").is_none());
    }

    #[test]
    fn volume_of_extracts_the_drive_letter() {
        assert_eq!(volume_of(Path::new(r"C:\data\source")).expect("ok"), "C:");
        assert_eq!(volume_of(Path::new(r"d:\backup")).expect("ok"), "D:");
    }

    #[test]
    fn volume_of_rejects_paths_without_a_drive_letter() {
        assert!(volume_of(Path::new(r"\\server\share\data")).is_err());
        assert!(volume_of(Path::new("relative/path")).is_err());
    }

    #[test]
    fn remap_to_shadow_joins_the_relative_path_onto_the_device_path() {
        let shadow = ShadowCopy {
            shadow_id: "{id}".to_string(),
            device_path: r"\\?\GLOBALROOT\Device\HarddiskVolumeShadowCopy12".to_string(),
        };
        let remapped = remap_to_shadow(Path::new(r"C:\data\source"), "C:", &shadow);
        assert_eq!(
            remapped,
            PathBuf::from(r"\\?\GLOBALROOT\Device\HarddiskVolumeShadowCopy12\data\source")
        );
    }

    #[test]
    fn remap_to_shadow_handles_the_volume_root_itself() {
        let shadow = ShadowCopy {
            shadow_id: "{id}".to_string(),
            device_path: r"\\?\GLOBALROOT\Device\HarddiskVolumeShadowCopy12".to_string(),
        };
        let remapped = remap_to_shadow(Path::new(r"C:\"), "C:", &shadow);
        assert_eq!(
            remapped,
            PathBuf::from(r"\\?\GLOBALROOT\Device\HarddiskVolumeShadowCopy12")
        );
    }
}
