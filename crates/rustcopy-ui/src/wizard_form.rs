//! The "Nuova attività" wizard's pure part (PIANO_GUI_ATTIVITA.md 3.2): turning what the person typed into
//! the core's [`WizardDraft`], saying on the step where it applies what is wrong, and writing the summary
//! shown before anything is created. What a task may and may not be is decided by
//! `robocopy_ingest::task_wizard`; this only reads text and builds sentences, so it is tested without a
//! window.

use std::path::{Path, PathBuf};

use robocopy_ingest::generations::BackupType;
use robocopy_ingest::integrity::HashAlgorithm;
use robocopy_ingest::task_wizard::{self, WizardDraft};

/// The number of steps.
pub const STEPS: i32 = 5;

/// What the person has typed or chosen so far, as the screens hold it.
#[derive(Debug, Clone, Default)]
pub struct Fields {
    pub name: String,
    pub sources: Vec<String>,
    pub dest: String,
    /// Index into [`KINDS`].
    pub kind: i32,
    pub verify: bool,
    /// One pattern per line.
    pub exclude_files: String,
    pub exclude_dirs: String,
    pub min_age: String,
    pub max_age: String,
    pub bandwidth: String,
}

/// The kinds of copy, in the order of the choice, with the sentence that says what each does.
pub const KINDS: [(&str, &str); 4] = [
    (
        "Copia semplice",
        "Aggiorna la destinazione: copia i file nuovi o cambiati e non cancella nulla.",
    ),
    (
        "Completa",
        "Ogni volta una copia di tutto, in una cartella nuova con la data.",
    ),
    (
        "Incrementale",
        "Una versione per volta, con solo le novità dall'ultima. La prima volta è una copia completa.",
    ),
    (
        "Differenziale",
        "Una versione con tutto ciò che è cambiato dall'ultima completa. La prima volta è una copia completa.",
    ),
];

pub fn backup_type(index: i32) -> Option<BackupType> {
    match index {
        1 => Some(BackupType::Full),
        2 => Some(BackupType::Incremental),
        3 => Some(BackupType::Differential),
        _ => None,
    }
}

/// The non-empty trimmed lines of a text box.
pub fn lines(text: &str) -> Vec<String> {
    text.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(str::to_string)
        .collect()
}

/// An optional whole number: empty is `None`, anything else must be a number.
fn number<T: std::str::FromStr>(text: &str, what: &str) -> Result<Option<T>, String> {
    let text = text.trim();
    if text.is_empty() {
        return Ok(None);
    }
    text.parse::<T>()
        .map(Some)
        .map_err(|_| format!("«{what}» dev'essere un numero intero, per esempio 7."))
}

/// The core's draft from the fields, or the sentence that says what to fix.
pub fn draft(fields: &Fields) -> Result<WizardDraft, String> {
    Ok(WizardDraft {
        name: fields.name.trim().to_string(),
        sources: fields.sources.iter().map(PathBuf::from).collect(),
        dest: PathBuf::from(fields.dest.trim()),
        backup_type: backup_type(fields.kind),
        verify: fields.verify.then(HashAlgorithm::default),
        exclude_files: lines(&fields.exclude_files),
        exclude_dirs: lines(&fields.exclude_dirs),
        min_age_days: number(&fields.min_age, "Età minima")?,
        max_age_days: number(&fields.max_age, "Età massima")?,
        bandwidth_limit_mbps: number(&fields.bandwidth, "Limite di banda")?,
    })
}

/// What is wrong on `step` (0-based), as a sentence, or `None` when the person can go on. The same sentences
/// the core says when the task is created, said earlier, on the step they apply to.
pub fn step_problem(step: i32, fields: &Fields, tasks_dir: &Path) -> Option<String> {
    match step {
        0 => {
            let name = fields.name.trim();
            if name.is_empty() {
                return Some("Dai un nome all'attività.".to_string());
            }
            if let Err(error) = robocopy_ingest::validate_job_name(name) {
                return Some(error.to_string());
            }
            if task_wizard::name_taken(tasks_dir, name) {
                return Some(format!(
                    "Esiste già un'attività chiamata «{name}»: scegli un altro nome."
                ));
            }
            if fields.sources.is_empty() {
                return Some("Scegli almeno una cartella o un file da copiare.".to_string());
            }
            None
        }
        1 => fields
            .dest
            .trim()
            .is_empty()
            .then(|| "Scegli la cartella di destinazione.".to_string()),
        3 => match draft(fields) {
            Err(sentence) => Some(sentence),
            Ok(draft) => task_wizard::settings_problem(&draft),
        },
        _ => None,
    }
}

/// The schedule as words, from the `--install-schedule` text the form built (`daily@02:00`, `hourly@6`,
/// `weekly@MON,WED@07:30`); `None` means "by hand".
pub fn when_text(spec: Option<&str>) -> String {
    let Some(spec) = spec else {
        return "Solo a mano".to_string();
    };
    const DAYS: [(&str, &str); 7] = [
        ("MON", "Lun"),
        ("TUE", "Mar"),
        ("WED", "Mer"),
        ("THU", "Gio"),
        ("FRI", "Ven"),
        ("SAT", "Sab"),
        ("SUN", "Dom"),
    ];
    let parts: Vec<&str> = spec.split('@').collect();
    match parts.as_slice() {
        ["daily", time] => format!("Ogni giorno alle {time}"),
        ["hourly", hours] => format!("Ogni {hours} ore"),
        ["weekly", days, time] => {
            let names: Vec<&str> = days
                .split(',')
                .map(|code| {
                    DAYS.iter()
                        .find(|(c, _)| c == &code)
                        .map_or(code, |(_, name)| *name)
                })
                .collect();
            format!("{} alle {time}", names.join(", "))
        }
        _ => spec.to_string(),
    }
}

/// The summary shown on the last step: everything that will be written, in words.
pub fn summary(fields: &Fields, when: &str) -> String {
    let (kind, _) = KINDS[usize::try_from(fields.kind)
        .unwrap_or(0)
        .min(KINDS.len() - 1)];
    let mut out = vec![
        format!("Nome: {}", fields.name.trim()),
        format!(
            "Cosa copiare: {}",
            match fields.sources.as_slice() {
                [one] => one.clone(),
                many => format!("{} elementi ({})", many.len(), many.join("; ")),
            }
        ),
        format!("Dove: {}", fields.dest.trim()),
        format!("Tipo: {kind}"),
        format!("Quando: {when}"),
        format!(
            "Verifica dopo la copia: {}",
            if fields.verify { "sì" } else { "no" }
        ),
    ];
    let files = lines(&fields.exclude_files);
    if !files.is_empty() {
        out.push(format!("File esclusi: {}", files.join(", ")));
    }
    let dirs = lines(&fields.exclude_dirs);
    if !dirs.is_empty() {
        out.push(format!("Cartelle escluse: {}", dirs.join(", ")));
    }
    match (fields.min_age.trim(), fields.max_age.trim()) {
        ("", "") => {}
        (min, "") => out.push(format!("Solo file modificati da almeno {min} giorni")),
        ("", max) => out.push(format!("Solo file modificati negli ultimi {max} giorni")),
        (min, max) => out.push(format!(
            "Solo file modificati da almeno {min} e al massimo {max} giorni"
        )),
    }
    if !fields.bandwidth.trim().is_empty() {
        out.push(format!("Limite di banda: {} MB/s", fields.bandwidth.trim()));
    }
    if matches!(fields.kind, 2 | 3) {
        out.push(
            "La prima esecuzione sarà una copia completa, come punto di partenza.".to_string(),
        );
    }
    out.push(
        "Mai: mirror, cancellazioni, conservazione dei cicli, comandi prima o dopo la copia."
            .to_string(),
    );
    out.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fields() -> Fields {
        Fields {
            name: "documenti".into(),
            sources: vec!["C:/Documenti".into()],
            dest: "D:/backup".into(),
            ..Fields::default()
        }
    }

    #[test]
    fn the_kinds_map_to_the_engines_backup_types() {
        assert_eq!(backup_type(0), None);
        assert_eq!(backup_type(1), Some(BackupType::Full));
        assert_eq!(backup_type(2), Some(BackupType::Incremental));
        assert_eq!(backup_type(3), Some(BackupType::Differential));
        assert_eq!(KINDS.len(), 4);
    }

    #[test]
    fn lines_drop_blanks_and_numbers_are_optional_but_not_wrong() {
        assert_eq!(lines(" *.tmp \n\n  cache\n"), vec!["*.tmp", "cache"]);
        let mut f = fields();
        f.min_age = "abc".into();
        assert!(draft(&f).expect_err("not a number").contains("Età minima"));
        f.min_age = "  ".into();
        assert!(draft(&f).is_ok(), "empty means not set");
        f.min_age = "7".into();
        assert_eq!(draft(&f).expect("draft").min_age_days, Some(7));
    }

    #[test]
    fn verification_is_the_default_algorithm_or_nothing() {
        let mut f = fields();
        assert_eq!(draft(&f).expect("draft").verify, None);
        f.verify = true;
        assert_eq!(
            draft(&f).expect("draft").verify,
            Some(HashAlgorithm::default())
        );
    }

    #[test]
    fn each_step_says_what_is_missing_before_going_on() {
        let dir = tempfile::tempdir().expect("tempdir");
        let tasks = dir.path();
        let mut f = fields();
        f.name.clear();
        assert!(step_problem(0, &f, tasks)
            .expect("problem")
            .contains("nome"));
        f.name = "a/b".into();
        assert!(step_problem(0, &f, tasks).is_some(), "an unusable name");
        f.name = "documenti".into();
        f.sources.clear();
        assert!(step_problem(0, &f, tasks)
            .expect("problem")
            .contains("almeno una cartella"));
        f.sources.push("C:/x".into());
        assert_eq!(step_problem(0, &f, tasks), None);

        f.dest.clear();
        assert!(step_problem(1, &f, tasks)
            .expect("problem")
            .contains("destinazione"));
        f.dest = "D:/b".into();
        assert_eq!(step_problem(1, &f, tasks), None);
        assert_eq!(
            step_problem(2, &f, tasks),
            None,
            "the schedule has its own check"
        );

        f.kind = 1;
        f.min_age = "10".into();
        f.max_age = "5".into();
        assert!(step_problem(3, &f, tasks)
            .expect("problem")
            .contains("età minima"));
    }

    #[test]
    fn a_name_already_used_is_refused_on_the_first_step() {
        let dir = tempfile::tempdir().expect("tempdir");
        std::fs::create_dir_all(dir.path().join("documenti")).expect("dir");
        std::fs::write(dir.path().join("documenti/documenti.toml"), "x").expect("file");
        let problem = step_problem(0, &fields(), dir.path()).expect("problem");
        assert!(problem.contains("Esiste già"), "{problem}");
    }

    #[test]
    fn the_schedule_reads_in_plain_italian() {
        assert_eq!(when_text(None), "Solo a mano");
        assert_eq!(when_text(Some("daily@02:00")), "Ogni giorno alle 02:00");
        assert_eq!(when_text(Some("hourly@6")), "Ogni 6 ore");
        assert_eq!(
            when_text(Some("weekly@MON,WED,SUN@07:30")),
            "Lun, Mer, Dom alle 07:30"
        );
        assert_eq!(
            when_text(Some("strano")),
            "strano",
            "an unknown shape is shown as it is"
        );
    }

    #[test]
    fn the_summary_says_everything_that_will_be_written_and_what_never_will() {
        let mut f = fields();
        f.kind = 2;
        f.verify = true;
        f.exclude_files = "*.tmp".into();
        f.max_age = "90".into();
        f.bandwidth = "20".into();
        let text = summary(&f, "Ogni giorno alle 22:00");
        for expected in [
            "Nome: documenti",
            "Cosa copiare: C:/Documenti",
            "Dove: D:/backup",
            "Tipo: Incrementale",
            "Quando: Ogni giorno alle 22:00",
            "Verifica dopo la copia: sì",
            "File esclusi: *.tmp",
            "negli ultimi 90 giorni",
            "Limite di banda: 20 MB/s",
            "La prima esecuzione sarà una copia completa",
            "Mai: mirror",
            "conservazione dei cicli",
        ] {
            assert!(text.contains(expected), "{expected}:\n{text}");
        }
        f.kind = 0;
        assert!(!summary(&f, "Solo a mano").contains("prima esecuzione"));
        f.sources.push("C:/Foto".into());
        assert!(summary(&f, "x").contains("2 elementi"));
    }
}
