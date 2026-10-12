//! `--refuse-dest-links`: does the destination contain a junction or a symbolic link?
//!
//! Why this exists: `robocopy.exe` **writes through** a junction in the destination. Verified against
//! the real binary, with and without `/XJ`: with `dest\sub` a junction to `other`, a copy of `sub\f.txt`
//! lands in `other\f.txt`. `/XJ` only decides what robocopy reads from the *source*. For an ordinary run
//! that is the person's own business, but an elevated schedule (`--schedule-elevated`) writes with
//! administrator rights into a folder somebody else may be able to change: whoever can create a
//! junction there can make the elevated process write somewhere they could not.
//!
//! The check is therefore three things, in this order, and any link found refuses the run:
//!
//! 1. the destination itself;
//! 2. every existing parent of it (a junction at `D:\Backups` redirects everything below it);
//! 3. everything already inside it (a junction left in the tree, to be written through on the next run).
//!
//! It runs before the copy and again after it, because the first run cannot see what is created while
//! the copy is going on; the second one cannot prevent that write, only refuse to call the run a success.
//! A junction made *during* the copy is the declared limit of this check — closing it needs the copy
//! itself to open every directory without following links, which `robocopy.exe` does not offer.

use std::path::{Path, PathBuf};

use crate::errors::IngestError;

/// Whether `metadata` (read **without** following the entry) describes a junction or a symbolic link.
fn is_link(metadata: &std::fs::Metadata) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        crate::elevation::is_reparse(metadata.file_attributes())
    }
    #[cfg(not(windows))]
    {
        metadata.file_type().is_symlink()
    }
}

/// The first junction or link on the way to `dest`, in `dest` or under it, if there is one.
pub fn find_link(dest: &Path) -> std::io::Result<Option<PathBuf>> {
    // The destination and each of its parents, from the top down. Drive and UNC prefixes and the root
    // are not folders someone made, so they are not looked at; a path that does not exist yet cannot
    // be a link, and neither can anything below it.
    let mut current = PathBuf::new();
    for component in dest.components() {
        current.push(component);
        if matches!(
            component,
            std::path::Component::Prefix(_) | std::path::Component::RootDir
        ) {
            continue;
        }
        match std::fs::symlink_metadata(&current) {
            Ok(metadata) if is_link(&metadata) => return Ok(Some(current)),
            Ok(_) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(e),
        }
    }
    // Everything already inside. `follow_links(false)`: a link is reported, never entered.
    for entry in walkdir::WalkDir::new(dest).follow_links(false).min_depth(1) {
        let entry = entry.map_err(std::io::Error::from)?;
        let metadata = entry.metadata().map_err(std::io::Error::from)?;
        if is_link(&metadata) {
            return Ok(Some(entry.into_path()));
        }
    }
    Ok(None)
}

/// [`find_link`] as the error the run stops with. Failing to read part of the tree is also a refusal:
/// a check that could not look is not a check that passed.
pub fn ensure_no_links(dest: &Path) -> Result<(), IngestError> {
    match find_link(dest) {
        Ok(None) => Ok(()),
        Ok(Some(path)) => Err(IngestError::DestinationHasLink {
            path: path.display().to_string(),
        }),
        Err(source) => Err(IngestError::io(dest, source)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A directory link, however this platform makes one without special rights.
    fn link_dir(link: &Path, target: &Path) {
        #[cfg(windows)]
        {
            let made = std::process::Command::new("cmd")
                .args(["/C", "mklink", "/J"])
                .arg(link)
                .arg(target)
                .output()
                .expect("cmd");
            assert!(
                made.status.success(),
                "{}",
                String::from_utf8_lossy(&made.stdout)
            );
        }
        #[cfg(not(windows))]
        std::os::unix::fs::symlink(target, link).expect("symlink");
    }

    #[test]
    fn a_plain_tree_passes() {
        let base = tempfile::tempdir().expect("tempdir");
        let dest = base.path().join("dest");
        std::fs::create_dir_all(dest.join("a/b")).expect("tree");
        std::fs::write(dest.join("a/b/f.txt"), b"x").expect("file");
        assert_eq!(find_link(&dest).expect("walk"), None);
        assert!(ensure_no_links(&dest).is_ok());
    }

    #[test]
    fn a_destination_that_does_not_exist_yet_passes() {
        let base = tempfile::tempdir().expect("tempdir");
        assert_eq!(
            find_link(&base.path().join("not/made/yet")).expect("walk"),
            None
        );
    }

    #[test]
    fn a_link_inside_the_tree_is_found_and_not_entered() {
        let base = tempfile::tempdir().expect("tempdir");
        let dest = base.path().join("dest");
        let other = base.path().join("other");
        std::fs::create_dir_all(&dest).expect("dest");
        std::fs::create_dir_all(&other).expect("other");
        link_dir(&dest.join("sub"), &other);
        assert_eq!(find_link(&dest).expect("walk"), Some(dest.join("sub")));
        let error = ensure_no_links(&dest).expect_err("refused");
        assert!(matches!(error, IngestError::DestinationHasLink { .. }));
        assert!(error.to_string().contains("sub"), "{error}");
    }

    #[test]
    fn a_destination_that_is_itself_a_link_is_found() {
        let base = tempfile::tempdir().expect("tempdir");
        let other = base.path().join("other");
        let dest = base.path().join("dest");
        std::fs::create_dir_all(&other).expect("other");
        link_dir(&dest, &other);
        assert_eq!(find_link(&dest).expect("walk"), Some(dest));
    }

    #[test]
    fn a_link_among_the_parents_is_found_even_below_it() {
        let base = tempfile::tempdir().expect("tempdir");
        let real = base.path().join("real");
        let link = base.path().join("link");
        std::fs::create_dir_all(real.join("deep")).expect("real");
        link_dir(&link, &real);
        // The destination is reached *through* the link and does not exist as a link itself.
        assert_eq!(
            find_link(&link.join("deep")).expect("walk"),
            Some(link.clone())
        );
        // And a destination that does not exist yet below the link is refused all the same.
        assert_eq!(find_link(&link.join("not/yet")).expect("walk"), Some(link));
    }

    #[cfg(unix)]
    #[test]
    fn a_symbolic_link_to_a_file_is_found_too() {
        let base = tempfile::tempdir().expect("tempdir");
        let dest = base.path().join("dest");
        std::fs::create_dir_all(&dest).expect("dest");
        std::fs::write(base.path().join("t.txt"), b"x").expect("target");
        std::os::unix::fs::symlink(base.path().join("t.txt"), dest.join("l.txt")).expect("link");
        assert_eq!(find_link(&dest).expect("walk"), Some(dest.join("l.txt")));
    }
}
