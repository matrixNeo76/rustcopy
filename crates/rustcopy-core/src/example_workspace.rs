//! F79: a minimal, safe example workspace for an operator with no configuration to start from.
//!
//! The installer never ships `examples/` (verified in `installer/rustcopy.iss`'s `[Files]`
//! section, `PIANO_GUI.md` §18.11) -- every in-product pointer to `examples/demo-locale.toml`
//! (`Jobs.svelte`'s empty state, `Help.svelte`'s "Da dove si comincia") is unreachable for anyone
//! who installed rustcopy rather than cloning the repository. This generates the same kind of
//! thing on the spot, in a location the operator already owns.
//!
//! Kept apart from [`crate::job_editor`] (F54) and [`crate::crypto`] (F56) deliberately, for the
//! same reason those two are their own modules rather than folded into [`crate::gui_api`]:
//! `gui_api` is documented read-only, and a write capability dissolving into it would blur a
//! boundary this crate otherwise keeps visible in the file tree. This module writes real file
//! *content*, not a configuration proposal or a credential -- a third, distinct category, so it
//! earns its own file rather than joining either of the other two.

use std::path::{Path, PathBuf};

use crate::atomic_write;
use crate::errors::IngestError;

/// Relative to the generated `source/` folder. Small and few on purpose: this exists to prove the
/// console works, not to teach every flag `examples/demo-locale.toml` already covers at length.
const SOURCE_FILES: &[(&str, &str)] = &[
    (
        "leggimi.txt",
        "Questo file esiste solo per far girare l'esempio generato da rustcopy.\n",
    ),
    ("documenti/nota.txt", "Nota di prova.\n"),
];

const CONFIG_TEMPLATE: &str = "\
# Esempio generato da rustcopy.
#
# Copia solo i file nuovi o cambiati dalla cartella \"source\" qui accanto verso \"dest\" -- nessun
# mirror, non cancella mai nulla. Riesegui questo file una seconda volta: non ricopia nulla, perché
# sorgente e destinazione sono già allineate.

source = \"source\"
dest   = \"dest\"
verify_integrity = true
";

/// Writes a working example under `target_dir` -- a `source/` tree with a couple of placeholder
/// files and a TOML pointing at it -- and returns the TOML's path.
///
/// Refuses if `target_dir` already exists: `std::fs::create_dir` (not `create_dir_all`) is the
/// refusal itself, the same OS-enforced atomicity `job_editor::propose_config` uses via
/// `create_new` rather than a separate exists-check-then-write that could race. A second click
/// here must never quietly discard what the first one produced -- an operator who has since edited
/// the generated example loses nothing.
pub fn create_example_workspace(target_dir: &Path) -> Result<PathBuf, IngestError> {
    std::fs::create_dir(target_dir).map_err(|error| {
        if error.kind() == std::io::ErrorKind::AlreadyExists {
            IngestError::ExampleWorkspaceAlreadyExists(target_dir.to_path_buf())
        } else {
            IngestError::io(target_dir, error)
        }
    })?;

    let source_dir = target_dir.join("source");
    for (relative, contents) in SOURCE_FILES {
        let path = source_dir.join(relative);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|error| IngestError::io(parent, error))?;
        }
        atomic_write(&path, contents.as_bytes()).map_err(|error| IngestError::io(&path, error))?;
    }

    let toml_path = target_dir.join("esempio.toml");
    atomic_write(&toml_path, CONFIG_TEMPLATE.as_bytes())
        .map_err(|error| IngestError::io(&toml_path, error))?;

    Ok(toml_path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writes_a_toml_that_points_at_real_files() {
        let dir = tempfile::tempdir().expect("tempdir");
        let target = dir.path().join("rustcopy-demo");

        let toml_path = create_example_workspace(&target).expect("first call succeeds");

        assert_eq!(toml_path, target.join("esempio.toml"));
        let toml = std::fs::read_to_string(&toml_path).expect("toml readable");
        assert!(toml.contains("source = \"source\""));
        assert!(toml.contains("dest   = \"dest\""));

        for (relative, contents) in SOURCE_FILES {
            let written = std::fs::read_to_string(target.join("source").join(relative))
                .unwrap_or_else(|error| panic!("{relative} must exist and be readable: {error}"));
            assert_eq!(&written, contents);
        }
    }

    /// The exact scenario the refusal exists for: an operator generates the example, edits it,
    /// then clicks the button again by mistake. The edit must survive.
    #[test]
    fn refuses_to_overwrite_an_existing_workspace() {
        let dir = tempfile::tempdir().expect("tempdir");
        let target = dir.path().join("rustcopy-demo");

        create_example_workspace(&target).expect("first call succeeds");
        let toml_path = target.join("esempio.toml");
        std::fs::write(&toml_path, "# edited by the operator\n").expect("simulate an edit");

        let error = create_example_workspace(&target).expect_err("second call must be refused");
        assert!(
            matches!(error, IngestError::ExampleWorkspaceAlreadyExists(ref path) if path == &target),
            "got {error:?}"
        );
        // The refusal is not just an error: the edit made after the first call must be intact.
        let toml = std::fs::read_to_string(&toml_path).expect("toml still readable");
        assert_eq!(toml, "# edited by the operator\n");
    }
}
