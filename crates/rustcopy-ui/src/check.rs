//! The sentences "Controlla prima" shows (CATALOGO_COMPORTAMENTI_GUI.md C03). The numbers and the
//! verdicts come from `gui_api::check_copy`; this only turns them into text, so it is a pure,
//! unit-tested function.

use robocopy_ingest::gui_api::CopyCheck;

use crate::format::{folder_name, human_bytes};

/// How the result should be drawn.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tone {
    /// Nothing in the way.
    Fine,
    /// Something a person should read before pressing Copia (never red: the copy may still be right).
    Attention,
}

/// The text of a check and its tone.
pub fn describe(check: &CopyCheck) -> (String, Tone) {
    if let Some(problem) = &check.problem {
        return (problem.clone(), Tone::Attention);
    }
    let mut tone = Tone::Fine;
    let mut lines: Vec<String> = Vec::new();

    for source in &check.sources {
        if !source.exists {
            tone = Tone::Attention;
            lines.push(format!("«{}» non esiste più.", folder_name(&source.path)));
        } else if !source.is_dir && !source.is_file {
            tone = Tone::Attention;
            lines.push(format!(
                "«{}» non è una cartella.",
                folder_name(&source.path)
            ));
        }
    }

    let folders = check.sources.iter().filter(|s| s.is_dir).count();
    let single_files = check.sources.iter().filter(|s| s.is_file).count();
    let mut places = Vec::new();
    if folders > 0 {
        places.push(format!(
            "{} {}",
            folders,
            if folders == 1 { "cartella" } else { "cartelle" }
        ));
    }
    if single_files > 0 {
        places.push(format!(
            "{} {}",
            single_files,
            if single_files == 1 {
                "file scelto"
            } else {
                "file scelti"
            }
        ));
    }
    lines.push(format!(
        "{} file, {} in {}.",
        check.total_files,
        human_bytes(check.total_bytes),
        places.join(" e ")
    ));
    lines.push(
        "Se la destinazione ha già file uguali non vengono riscritti: la copia può pesare meno."
            .to_string(),
    );

    match (check.free_bytes, check.enough_space) {
        (Some(free), Some(true)) => lines.push(format!(
            "Spazio libero nella destinazione: {} — basta.",
            human_bytes(free)
        )),
        (Some(free), Some(false)) => {
            tone = Tone::Attention;
            lines.push(format!(
                "Spazio libero nella destinazione: {} — non basta per {}.",
                human_bytes(free),
                human_bytes(check.total_bytes)
            ));
        }
        _ => lines.push("Lo spazio libero della destinazione non si può verificare.".to_string()),
    }
    (lines.join("\n"), tone)
}

#[cfg(test)]
mod tests {
    use super::*;
    use robocopy_ingest::gui_api::SourceCheck;

    fn source(path: &str, exists: bool, is_dir: bool, files: u64, bytes: u64) -> SourceCheck {
        SourceCheck {
            path: path.to_string(),
            exists,
            is_dir,
            is_file: false,
            files,
            bytes,
        }
    }

    fn check(sources: Vec<SourceCheck>, free: Option<u64>, enough: Option<bool>) -> CopyCheck {
        CopyCheck {
            total_files: sources.iter().map(|s| s.files).sum(),
            total_bytes: sources.iter().map(|s| s.bytes).sum(),
            sources,
            free_bytes: free,
            enough_space: enough,
            problem: None,
        }
    }

    #[test]
    fn a_plain_copy_with_room_is_fine() {
        let (text, tone) = describe(&check(
            vec![source(r"C:\foto", true, true, 3, 3 * 1024 * 1024)],
            Some(10 * 1024 * 1024 * 1024),
            Some(true),
        ));
        assert_eq!(tone, Tone::Fine);
        assert!(text.contains("3 file, 3,0 MB in 1 cartella."), "{text}");
        assert!(text.contains("basta"), "{text}");
    }

    #[test]
    fn missing_folders_and_a_full_destination_both_ask_for_attention() {
        let (text, tone) = describe(&check(
            vec![
                source(r"C:\foto", true, true, 1, 100),
                source(r"E:\sparito", false, false, 0, 0),
            ],
            Some(50),
            Some(false),
        ));
        assert_eq!(tone, Tone::Attention);
        assert!(text.contains("«sparito» non esiste più."), "{text}");
        assert!(text.contains("non basta"), "{text}");
    }

    #[test]
    fn unknown_free_space_is_said_not_assumed() {
        let (text, tone) = describe(&check(
            vec![source(r"C:\foto", true, true, 1, 100)],
            None,
            None,
        ));
        assert_eq!(tone, Tone::Fine);
        assert!(text.contains("non si può verificare"), "{text}");
    }

    #[test]
    fn a_refusal_from_the_plan_is_shown_verbatim() {
        let mut refused = check(vec![], None, None);
        refused.problem = Some("Scegli almeno una cartella da copiare.".to_string());
        let (text, tone) = describe(&refused);
        assert_eq!(text, "Scegli almeno una cartella da copiare.");
        assert_eq!(tone, Tone::Attention);
    }
}
