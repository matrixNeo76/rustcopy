//! The sentence under a folder box in the editor after "Verifica" (CATALOGO_COMPORTAMENTI_GUI.md L40):
//! does this folder exist, and how much is in it. The numbers come from `gui_api::inspect_path`; this
//! only says them, so it is pure and unit-tested.

use robocopy_ingest::gui_api::PathInspection;

use crate::format::human_bytes;

/// Which box was checked: a missing destination is normal (the first copy creates it), a missing
/// source is a problem.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    Source,
    Dest,
}

/// What to say about `found` for a box of this role.
pub fn describe(role: Role, found: &PathInspection) -> String {
    if !found.exists {
        return match role {
            Role::Source => "Questa cartella non esiste.".to_string(),
            Role::Dest => "Non esiste ancora: verrà creata alla prima copia.".to_string(),
        };
    }
    if !found.is_dir {
        return "Esiste, ma non è una cartella.".to_string();
    }
    let contents = format!(
        "{} file, {} {}, {}",
        found.total_files,
        found.total_dirs,
        if found.total_dirs == 1 {
            "sottocartella"
        } else {
            "sottocartelle"
        },
        human_bytes(found.total_bytes)
    );
    match role {
        Role::Source => format!("Contiene {contents}."),
        Role::Dest => format!("Esiste già e contiene {contents}."),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn found(exists: bool, is_dir: bool, files: u64, dirs: u64, bytes: u64) -> PathInspection {
        PathInspection {
            exists,
            is_dir,
            total_files: files,
            total_dirs: dirs,
            total_bytes: bytes,
        }
    }

    #[test]
    fn a_missing_source_is_a_problem_and_a_missing_destination_is_normal() {
        let nothing = found(false, false, 0, 0, 0);
        assert_eq!(
            describe(Role::Source, &nothing),
            "Questa cartella non esiste."
        );
        assert!(describe(Role::Dest, &nothing).contains("verrà creata"));
    }

    #[test]
    fn a_file_where_a_folder_was_expected_is_named() {
        let file = found(true, false, 0, 0, 0);
        assert_eq!(
            describe(Role::Source, &file),
            "Esiste, ma non è una cartella."
        );
        assert_eq!(
            describe(Role::Dest, &file),
            "Esiste, ma non è una cartella."
        );
    }

    #[test]
    fn a_folder_says_how_much_is_in_it_and_the_destination_says_it_is_already_there() {
        let folder = found(true, true, 12, 3, 2048);
        let source = describe(Role::Source, &folder);
        assert!(source.starts_with("Contiene 12 file, 3 sottocartelle, "));
        assert!(describe(Role::Dest, &folder).starts_with("Esiste già e contiene 12 file"));
    }

    #[test]
    fn one_subfolder_is_singular() {
        let folder = found(true, true, 1, 1, 10);
        assert!(describe(Role::Source, &folder).contains("1 sottocartella,"));
    }
}
