//! The scheduling form's pure part (CATALOGO_COMPORTAMENTI_GUI.md L32): turning what the person typed into
//! the CLI's own `--install-schedule` grammar. Whether a schedule may be installed at all, and every
//! other refusal, belong to `robocopy_ingest::schedule::install_arguments`; this only builds the text,
//! so a typo is caught here with a sentence instead of by `schtasks.exe` later.

/// The weekday codes in the order of the form's check boxes.
pub const DAYS: [&str; 7] = ["MON", "TUE", "WED", "THU", "FRI", "SAT", "SUN"];

/// What kind of recurrence was chosen (the order of the combo box).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Recurrence {
    Daily,
    EveryHours,
    Weekly,
}

impl Recurrence {
    pub fn from_index(index: i32) -> Self {
        match index {
            1 => Recurrence::EveryHours,
            2 => Recurrence::Weekly,
            _ => Recurrence::Daily,
        }
    }
}

/// `HH:MM` with two digits each, hours 0-23 and minutes 0-59, or `None`.
fn clean_time(text: &str) -> Option<String> {
    let (hours, minutes) = text.trim().split_once(':')?;
    let ok = |part: &str, max: u32| {
        part.len() == 2
            && part.chars().all(|c| c.is_ascii_digit())
            && part.parse::<u32>().is_ok_and(|n| n <= max)
    };
    (ok(hours, 23) && ok(minutes, 59)).then(|| format!("{hours}:{minutes}"))
}

/// The `--install-schedule` value for the form, or the sentence that says what to fix.
pub fn build_spec(
    recurrence: Recurrence,
    time: &str,
    hours: &str,
    days: [bool; 7],
) -> Result<String, String> {
    let time_error =
        || "Scrivi l'ora come ore e minuti a due cifre, per esempio 02:30.".to_string();
    match recurrence {
        Recurrence::Daily => Ok(format!(
            "daily@{}",
            clean_time(time).ok_or_else(time_error)?
        )),
        Recurrence::EveryHours => {
            let n: u32 = hours
                .trim()
                .parse()
                .map_err(|_| "Scrivi ogni quante ore, con un numero da 1 a 23.".to_string())?;
            if !(1..=23).contains(&n) {
                return Err("Le ore devono essere da 1 a 23: per intervalli più lunghi scegli «Ogni giorno».".to_string());
            }
            Ok(format!("hourly@{n}"))
        }
        Recurrence::Weekly => {
            let chosen: Vec<&str> = DAYS
                .iter()
                .zip(days)
                .filter(|(_, on)| *on)
                .map(|(day, _)| *day)
                .collect();
            if chosen.is_empty() {
                return Err("Scegli almeno un giorno della settimana.".to_string());
            }
            Ok(format!(
                "weekly@{}@{}",
                chosen.join(","),
                clean_time(time).ok_or_else(time_error)?
            ))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_recurrence_builds_the_clis_own_grammar() {
        let none = [false; 7];
        assert_eq!(
            build_spec(Recurrence::Daily, " 02:30 ", "", none).as_deref(),
            Ok("daily@02:30")
        );
        assert_eq!(
            build_spec(Recurrence::EveryHours, "", " 6 ", none).as_deref(),
            Ok("hourly@6")
        );
        let mut days = none;
        days[0] = true;
        days[4] = true;
        assert_eq!(
            build_spec(Recurrence::Weekly, "03:00", "", days).as_deref(),
            Ok("weekly@MON,FRI@03:00")
        );
    }

    #[test]
    fn a_typo_gets_a_sentence_not_a_guess() {
        let none = [false; 7];
        for bad in ["", "2:30", "24:00", "12:60", "ab:cd", "0230"] {
            assert!(
                build_spec(Recurrence::Daily, bad, "", none).is_err(),
                "{bad}"
            );
        }
        for bad in ["", "0", "24", "x", "-1"] {
            assert!(
                build_spec(Recurrence::EveryHours, "", bad, none).is_err(),
                "{bad}"
            );
        }
        assert!(
            build_spec(Recurrence::Weekly, "02:00", "", none).is_err(),
            "no day chosen"
        );
    }

    #[test]
    fn what_this_builds_the_core_parser_accepts() {
        let mut days = [false; 7];
        days[2] = true;
        for spec in [
            build_spec(Recurrence::Daily, "23:59", "", days),
            build_spec(Recurrence::EveryHours, "", "23", days),
            build_spec(Recurrence::Weekly, "00:00", "", days),
        ] {
            let spec = spec.expect("built");
            assert!(
                robocopy_ingest::schedule::parse_schedule_spec(&spec).is_ok(),
                "{spec}"
            );
        }
    }
}
