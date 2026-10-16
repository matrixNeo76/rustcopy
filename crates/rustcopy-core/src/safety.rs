//! The console's safety level (GUI Slint plan, phase 6; SPEC_GUI_SLINT.md 10.1, RF-Y12).
//!
//! What the console may *prepare* depends on a level the person chose, not on any job file:
//!
//! * **Prudent** (the default): starts copies, shows, reports. Nothing destructive, nothing scheduled.
//! * **Standard**: also prepares schedules (the CLI installs them after Windows' own elevation prompt)
//!   and the two-step *move*.
//! * **Expert**: also mirrors and purges **attended** (preview and confirmation every time). An
//!   unattended mirror or purge is never something the console starts, at any level: it stays a
//!   command-line decision.
//!
//! Four rules keep this robust instead of a mere switch:
//!
//! 1. Anything that cannot be read -- no file, a damaged one, a level this version does not know --
//!    is [`SafetyLevel::Prudent`]. A reset or an upgrade can only make the console more careful.
//! 2. The level lives in the person's data directory ([`crate::sessions::data_dir`]), **never in a job
//!    or configuration file**: a `.toml` that travels between machines cannot raise it.
//! 3. Raising needs an explicit confirmation ([`Confirmation::Confirmed`]); lowering never does.
//! 4. Every change is appended to `safety.log`, so what the level was, and when, can be checked later.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::errors::IngestError;

const SETTINGS_FILE: &str = "settings.json";
const LOG_FILE: &str = "safety.log";

/// How much the console may prepare. Ordered from the most careful to the most permissive.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default, Serialize, Deserialize,
)]
#[serde(rename_all = "lowercase")]
pub enum SafetyLevel {
    #[default]
    Prudent,
    Standard,
    Expert,
}

/// A capability a level can unlock, named so the interface can ask "what level does this need?" instead
/// of hard-coding the answer next to each button.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Feature {
    /// A schedule installed from the console.
    Schedule,
    /// A schedule that runs with administrator rights and a shadow copy of the disk.
    ElevatedSchedule,
    /// The two-step move: copy, verify, confirm, delete the original.
    Move,
    /// A run that rebuilds a full generation.
    ForceFull,
    /// A command that runs before or after a job's copy, written from the job editor. It is the one thing the
    /// console writes that **executes** later, on every run and unattended, so it sits at the highest level.
    JobCommands,
}

impl Feature {
    /// The lowest level that unlocks the feature. The one place this is decided: every `can_*` below and
    /// every message in the interface derive from it, so the greyed button and the refusal in the core
    /// cannot disagree.
    pub fn required_level(self) -> SafetyLevel {
        match self {
            Feature::Schedule | Feature::Move | Feature::ForceFull => SafetyLevel::Standard,
            Feature::ElevatedSchedule | Feature::JobCommands => SafetyLevel::Expert,
        }
    }
}

impl SafetyLevel {
    /// Whether this level unlocks `feature`.
    pub fn allows(self, feature: Feature) -> bool {
        self >= feature.required_level()
    }

    /// Why `feature` is unavailable at this level, as the sentence shown beside it; `None` when it is
    /// available. The interface greys the control when this is `Some` and prints the text.
    pub fn lock_reason(self, feature: Feature) -> Option<String> {
        if self.allows(feature) {
            None
        } else {
            Some(format!(
                "Richiede il livello {}",
                feature.required_level().name()
            ))
        }
    }

    /// The name shown to the person.
    pub fn name(self) -> &'static str {
        match self {
            SafetyLevel::Prudent => "Prudente",
            SafetyLevel::Standard => "Standard",
            SafetyLevel::Expert => "Esperto",
        }
    }

    /// What the level lets the person do and what it does not, in full: shown on the safety page for
    /// the level picked and again in the confirmation that raises it.
    ///
    /// It describes what the console **actually offers** today, not what the level is reserved for: an
    /// earlier wording promised "attended mirror and purges" at Expert, which the console never offers
    /// at any level (`can_run_attended_purge` has no caller), and left out the one thing Expert really
    /// unlocks. A description that promises more than the button does is worse than a short one.
    pub fn describe(self) -> &'static str {
        match self {
            SafetyLevel::Prudent => "Con questo livello puoi:
  • avviare una copia da «Nuova copia» e seguirne l'avanzamento;
  • riprendere, ripetere o salvare come attività una copia;
  • leggere report, storico e registro, e vedere l'anteprima di un ripristino;
  • far verificare i file copiati.

Con questo livello non puoi: preparare pianificazioni, usare «Sposta» (che cancella gli originali dopo averli copiati) né «Forza completo» (che rifà da capo una copia a generazioni).",
            SafetyLevel::Standard => "Come Prudente, e in più puoi:
  • preparare una pianificazione: un'attività di Utilità di pianificazione di Windows che avvia la copia all'ora scelta, con il tuo utente e solo quando sei connesso;
  • usare «Sposta»: copia, verifica e, solo dopo la tua conferma, cancella gli originali che hanno una copia identica;
  • usare «Forza completo»: rifà una copia completa invece di una incrementale. Non cancella nulla, ma ricopia tutto.

Non puoi ancora: preparare pianificazioni con privilegi elevati, né scrivere nell'editor un comando da eseguire prima o dopo la copia.",
            SafetyLevel::Expert => "Come Standard, e in più puoi:
  • preparare una pianificazione con privilegi elevati: la copia parte da sola con diritti da amministratore e una copia ombra del disco (VSS), così copia anche i file aperti. Windows chiede conferma una volta sola, quando la crei;
  • scrivere nell'editor di un'attività un comando da eseguire prima o dopo la copia. Gira con i tuoi diritti a ogni esecuzione, anche in automatico, quindi ti mostra il testo esatto e chiede una conferma. Togliere un comando si può a ogni livello.

Una pianificazione elevata copia soltanto: niente mirror, niente comandi prima o dopo, solo percorsi completi. Mirror e pulizie non partono mai in automatico dalla console, a nessun livello, e oggi non sono ancora disponibili nemmeno con la tua conferma: restano un comando da scrivere a mano.",
        }
    }

    /// Whether the console may prepare a schedule (the CLI installs it, after UAC).
    pub fn can_prepare_schedules(self) -> bool {
        self.allows(Feature::Schedule)
    }

    /// Whether the console may prepare a schedule that runs **elevated** (administrator rights without a
    /// prompt each time, for VSS). It is the one schedule that runs with more rights than the person has
    /// day to day, so it sits at the highest level.
    pub fn can_prepare_elevated_schedules(self) -> bool {
        self.allows(Feature::ElevatedSchedule)
    }

    /// Whether the console may start a run that forces a full generation (`--force-full`). It deletes
    /// nothing, but it copies everything again, so it is behind the same level as a schedule rather
    /// than one click away from a first-time user.
    pub fn can_force_full(self) -> bool {
        self.allows(Feature::ForceFull)
    }

    /// Whether the job editor may write a command to run before or after a copy (and only with the person's
    /// confirmation of its exact text, see `job_editor::CommandPermit`). Removing a command needs no level.
    pub fn can_edit_job_commands(self) -> bool {
        self.allows(Feature::JobCommands)
    }

    /// Whether the two-step *move* (copy, verify, confirm, delete the original) is available.
    pub fn can_move(self) -> bool {
        self.allows(Feature::Move)
    }

    /// Whether an attended mirror or purge, with preview and confirmation, may be started.
    pub fn can_run_attended_purge(self) -> bool {
        self >= SafetyLevel::Expert
    }

    /// Whether the console may start an unattended mirror or purge. **Never**, at any level: kept as a
    /// function so the rule has a name and a test, not as a comment.
    pub fn can_run_unattended_purge(self) -> bool {
        false
    }

    /// The level named by `text`, if it is one of the three.
    pub fn parse(text: &str) -> Option<Self> {
        match text.trim().to_lowercase().as_str() {
            "prudent" | "prudente" => Some(SafetyLevel::Prudent),
            "standard" => Some(SafetyLevel::Standard),
            "expert" | "esperto" => Some(SafetyLevel::Expert),
            _ => None,
        }
    }
}

/// Whether the person confirmed the change, after being told what it unlocks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Confirmation {
    Confirmed,
    NotConfirmed,
}

#[derive(Serialize, Deserialize)]
struct Stored {
    level: SafetyLevel,
}

fn settings_path(dir: &Path) -> PathBuf {
    dir.join(SETTINGS_FILE)
}

/// The level stored in `dir`, [`SafetyLevel::Prudent`] when it cannot be read as one of the three.
pub fn load(dir: &Path) -> SafetyLevel {
    std::fs::read_to_string(settings_path(dir))
        .ok()
        // A file written by hand may start with a byte-order mark.
        .map(|text| text.trim_start_matches('\u{feff}').to_string())
        .and_then(|text| serde_json::from_str::<Stored>(&text).ok())
        .map_or(SafetyLevel::Prudent, |stored| stored.level)
}

/// Sets the level in `dir` and records the change. Raising without [`Confirmation::Confirmed`] is
/// refused; lowering, or keeping the same level, never needs one. Returns the level now in force.
pub fn set(
    dir: &Path,
    target: SafetyLevel,
    confirmation: Confirmation,
) -> Result<SafetyLevel, IngestError> {
    let current = load(dir);
    if target == current {
        return Ok(current);
    }
    if target > current && confirmation != Confirmation::Confirmed {
        return Err(IngestError::CopyPlanInvalid(
            "Alzare il livello di sicurezza richiede una conferma.".to_string(),
        ));
    }
    std::fs::create_dir_all(dir).map_err(|e| IngestError::io(dir, e))?;
    let text = serde_json::to_string_pretty(&Stored { level: target })
        .map_err(|e| IngestError::Crypto(e.to_string()))?;
    crate::atomic_write(&settings_path(dir), text.as_bytes())
        .map_err(|e| IngestError::io(settings_path(dir), e))?;
    append_log(dir, current, target);
    Ok(target)
}

/// One line per change: when, from, to. A failure to log never undoes the change: the level in force
/// is what the file says, and the log is the record, not the source.
fn append_log(dir: &Path, from: SafetyLevel, to: SafetyLevel) {
    use std::io::Write;
    let line = format!(
        "{}\t{}\t{}\n",
        chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
        from.name(),
        to.name()
    );
    if let Ok(mut file) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(dir.join(LOG_FILE))
    {
        let _ = file.write_all(line.as_bytes());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nothing_readable_means_prudent() {
        let dir = tempfile::tempdir().expect("tempdir");
        assert_eq!(load(dir.path()), SafetyLevel::Prudent, "no file");
        std::fs::write(dir.path().join(SETTINGS_FILE), "not json").expect("write");
        assert_eq!(load(dir.path()), SafetyLevel::Prudent, "damaged file");
        std::fs::write(dir.path().join(SETTINGS_FILE), r#"{"level":"omnipotent"}"#).expect("write");
        assert_eq!(
            load(dir.path()),
            SafetyLevel::Prudent,
            "a level this version does not know"
        );
    }

    #[test]
    fn a_byte_order_mark_does_not_hide_a_stored_level() {
        let dir = tempfile::tempdir().expect("tempdir");
        std::fs::write(
            dir.path().join(SETTINGS_FILE),
            "\u{feff}{\"level\":\"standard\"}",
        )
        .expect("write");
        assert_eq!(load(dir.path()), SafetyLevel::Standard);
    }

    #[test]
    fn raising_needs_a_confirmation_and_lowering_does_not() {
        let dir = tempfile::tempdir().expect("tempdir");
        assert!(set(
            dir.path(),
            SafetyLevel::Standard,
            Confirmation::NotConfirmed
        )
        .is_err());
        assert_eq!(
            load(dir.path()),
            SafetyLevel::Prudent,
            "refused, nothing changed"
        );

        assert_eq!(
            set(dir.path(), SafetyLevel::Expert, Confirmation::Confirmed).expect("raise"),
            SafetyLevel::Expert
        );
        assert_eq!(
            set(dir.path(), SafetyLevel::Prudent, Confirmation::NotConfirmed).expect("lower"),
            SafetyLevel::Prudent
        );
        assert_eq!(load(dir.path()), SafetyLevel::Prudent);
    }

    #[test]
    fn every_change_is_logged_and_a_no_op_is_not() {
        let dir = tempfile::tempdir().expect("tempdir");
        set(dir.path(), SafetyLevel::Prudent, Confirmation::NotConfirmed).expect("same level");
        assert!(
            !dir.path().join(LOG_FILE).exists(),
            "nothing changed, nothing logged"
        );

        set(dir.path(), SafetyLevel::Standard, Confirmation::Confirmed).expect("raise");
        set(dir.path(), SafetyLevel::Prudent, Confirmation::NotConfirmed).expect("lower");
        let log = std::fs::read_to_string(dir.path().join(LOG_FILE)).expect("log");
        let lines: Vec<&str> = log.lines().collect();
        assert_eq!(lines.len(), 2);
        assert!(lines[0].ends_with("Prudente\tStandard"), "{}", lines[0]);
        assert!(lines[1].ends_with("Standard\tPrudente"), "{}", lines[1]);
    }

    /// The description says what the level does, and each claim has a function behind it: what a level
    /// "can" do in words is what `can_*` says it can. It must also never promise a mirror or purge from the
    /// console, which no level offers (see `describe`).
    /// The `can_*` functions, the greyed-out text and the level a feature needs all come from one
    /// answer, so a feature moved to another level moves everywhere at once.
    #[test]
    fn a_locked_feature_names_the_level_that_unlocks_it() {
        use SafetyLevel::*;
        let all = [
            Feature::Schedule,
            Feature::ElevatedSchedule,
            Feature::Move,
            Feature::ForceFull,
            Feature::JobCommands,
        ];
        for level in [Prudent, Standard, Expert] {
            for feature in all {
                assert_eq!(
                    level.lock_reason(feature).is_none(),
                    level.allows(feature),
                    "{level:?} {feature:?}"
                );
            }
        }
        assert_eq!(
            Prudent.lock_reason(Feature::Schedule).as_deref(),
            Some("Richiede il livello Standard")
        );
        assert_eq!(
            Standard.lock_reason(Feature::ElevatedSchedule).as_deref(),
            Some("Richiede il livello Esperto")
        );
        assert_eq!(Expert.lock_reason(Feature::ElevatedSchedule), None);
        assert_eq!(
            (
                Prudent.can_prepare_schedules(),
                Standard.can_move(),
                Standard.can_force_full()
            ),
            (false, true, true)
        );
    }

    #[test]
    fn the_description_matches_what_each_level_really_allows() {
        use SafetyLevel::*;
        for level in [Prudent, Standard, Expert] {
            let text = level.describe();
            assert!(text.lines().count() > 3, "{level:?} is described in full");
            assert!(
                !text.contains("mirror e pulizie presidiati"),
                "{level:?} must not promise an attended purge the console does not offer"
            );
        }
        // What each level names is exactly what its functions allow.
        assert!(Standard.describe().contains("Sposta") && Standard.can_move());
        assert!(Prudent.describe().contains("non puoi") && !Prudent.can_move());
        assert!(Standard.describe().contains("Forza completo") && Standard.can_force_full());
        assert!(
            Expert.describe().contains("privilegi elevati")
                && Expert.can_prepare_elevated_schedules()
        );
        assert!(
            Expert.describe().contains("comando") && Expert.can_edit_job_commands(),
            "Expert says it can write a command, and can"
        );
        assert!(
            Standard
                .describe()
                .contains("né scrivere nell'editor un comando")
                && !Standard.can_edit_job_commands(),
            "Standard says it cannot, and cannot"
        );
        assert!(
            Standard.describe().contains("Non puoi ancora")
                && !Standard.can_prepare_elevated_schedules(),
            "Standard says it cannot do the elevated schedule, and it cannot"
        );
        // The one thing no level does, said plainly at the top.
        assert!(Expert.describe().contains("a nessun livello"));
        assert!(!Expert.can_run_unattended_purge());
    }

    #[test]
    fn capabilities_grow_with_the_level_and_unattended_purge_never_exists() {
        use SafetyLevel::*;
        assert!(
            !Prudent.can_prepare_schedules()
                && !Prudent.can_move()
                && !Prudent.can_run_attended_purge()
        );
        assert!(
            Standard.can_prepare_schedules()
                && Standard.can_move()
                && !Standard.can_run_attended_purge()
        );
        assert!(
            Expert.can_prepare_schedules() && Expert.can_move() && Expert.can_run_attended_purge()
        );
        for level in [Prudent, Standard, Expert] {
            assert!(!level.can_run_unattended_purge(), "{level:?}");
        }
    }

    #[test]
    fn the_levels_parse_by_either_name_and_the_default_is_the_most_careful() {
        assert_eq!(SafetyLevel::default(), SafetyLevel::Prudent);
        assert_eq!(SafetyLevel::parse("Esperto"), Some(SafetyLevel::Expert));
        assert_eq!(
            SafetyLevel::parse(" standard "),
            Some(SafetyLevel::Standard)
        );
        assert_eq!(SafetyLevel::parse("?"), None);
    }
}
