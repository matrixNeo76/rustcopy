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

impl SafetyLevel {
    /// The name shown to the person.
    pub fn name(self) -> &'static str {
        match self {
            SafetyLevel::Prudent => "Prudente",
            SafetyLevel::Standard => "Standard",
            SafetyLevel::Expert => "Esperto",
        }
    }

    /// What this level adds on top of the one below, as sentences for the confirmation.
    pub fn unlocks(self) -> &'static str {
        match self {
            SafetyLevel::Prudent => "Solo avviare copie, mostrare e segnalare. Nessuna pianificazione né cancellazione dalla console.",
            SafetyLevel::Standard => "In più: preparare pianificazioni (le installa la riga di comando dopo la richiesta di Windows) e «Sposta», che copia, verifica e solo dopo la tua conferma cancella l'originale.",
            SafetyLevel::Expert => "In più: mirror e pulizie presidiati, con anteprima e conferma a ogni esecuzione. Un mirror o una pulizia non presidiati non partono mai dalla console, a nessun livello.",
        }
    }

    /// Whether the console may prepare a schedule (the CLI installs it, after UAC).
    pub fn can_prepare_schedules(self) -> bool {
        self >= SafetyLevel::Standard
    }

    /// Whether the two-step *move* (copy, verify, confirm, delete the original) is available.
    pub fn can_move(self) -> bool {
        self >= SafetyLevel::Standard
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
