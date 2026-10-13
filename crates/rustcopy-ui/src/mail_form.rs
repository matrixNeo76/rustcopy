//! The mail page's pure part (PIANO_GUI_ATTIVITA.md 3.5): turning what is typed into the core's
//! [`MailSettings`] and the answer of "Invia una prova" into a sentence. What the settings may be is decided
//! by `robocopy_ingest::mail_settings`; the password never passes through here as anything but a *name*
//! (the secret itself goes straight to Windows' Credential Manager), so it cannot end up in a file.

use robocopy_ingest::mail_settings::{self, MailSettings};

/// The protections, in the order of the choice box.
pub const SECURITIES: [&str; 3] = ["starttls", "tls", "none"];

/// What the screen holds.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Fields {
    pub enabled: bool,
    pub host: String,
    /// Empty = the protection's usual port.
    pub port: String,
    pub security: i32,
    pub username: String,
    /// The name of the saved password in Windows' Credential Manager.
    pub password_name: String,
    /// A password specification someone wrote by hand (`env:`/`file:`): shown, kept, never edited here.
    pub password_other: String,
    pub from: String,
    /// Recipients separated by commas, semicolons or line breaks.
    pub to: String,
    pub subject_prefix: String,
}

/// The recipients in a text box: split on commas, semicolons and line breaks, trimmed, without blanks.
pub fn recipients(text: &str) -> Vec<String> {
    text.split([',', ';', '\n', '\r'])
        .map(str::trim)
        .filter(|item| !item.is_empty())
        .map(str::to_string)
        .collect()
}

/// The core's settings from the screen, or the sentence that says what to fix.
pub fn settings(fields: &Fields) -> Result<MailSettings, String> {
    let port = match fields.port.trim() {
        "" => None,
        text => Some(
            text.parse::<u16>()
                .map_err(|_| "La porta deve essere un numero da 1 a 65535.".to_string())?,
        ),
    };
    let password_spec = if !fields.password_other.trim().is_empty() {
        fields.password_other.trim().to_string()
    } else if !fields.password_name.trim().is_empty() {
        format!("keyring:{}", fields.password_name.trim())
    } else {
        String::new()
    };
    Ok(MailSettings {
        enabled: fields.enabled,
        host: fields.host.trim().to_string(),
        port,
        security: SECURITIES
            .get(usize::try_from(fields.security).unwrap_or(0))
            .copied()
            .unwrap_or("starttls")
            .to_string(),
        username: fields.username.trim().to_string(),
        password_spec,
        from: fields.from.trim().to_string(),
        to: recipients(&fields.to),
        subject_prefix: fields.subject_prefix.trim().to_string(),
    })
}

/// The screen for the settings a file holds.
pub fn fields_from(settings: &MailSettings) -> Fields {
    let keyring = mail_settings::keyring_name(&settings.password_spec);
    Fields {
        enabled: settings.enabled,
        host: settings.host.clone(),
        port: settings.port.map(|p| p.to_string()).unwrap_or_default(),
        security: SECURITIES
            .iter()
            .position(|s| *s == settings.security)
            .map_or(0, |i| i as i32),
        username: settings.username.clone(),
        password_name: keyring.unwrap_or_default().to_string(),
        // A spec that is not `keyring:` (written by hand) is carried through untouched.
        password_other: if keyring.is_none() {
            settings.password_spec.clone()
        } else {
            String::new()
        },
        from: settings.from.clone(),
        to: settings.to.join(", "),
        subject_prefix: settings.subject_prefix.clone(),
    }
}

/// What "Invia una prova" came back with, as (it worked, the sentence to show). The server's own sentence is
/// kept (it names the recipient or the reason); an unknown code still says something.
pub fn test_outcome(exit_code: i32, output: &str) -> (bool, String) {
    let last = output
        .lines()
        .rev()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .unwrap_or_default();
    match exit_code {
        0 => (
            true,
            if last.is_empty() {
                "Messaggio di prova inviato.".to_string()
            } else {
                last.to_string()
            },
        ),
        1 | 2 if !last.is_empty() => (false, last.to_string()),
        1 => (
            false,
            "Il server di posta ha rifiutato il messaggio o non risponde.".to_string(),
        ),
        2 => (
            false,
            "Le impostazioni della posta non sono utilizzabili.".to_string(),
        ),
        code => (
            false,
            format!("La prova non è andata a buon fine (codice {code}). {last}")
                .trim()
                .to_string(),
        ),
    }
}

/// Whether the notification server is there, in words, with what to do when it is not: the mail only
/// reaches anyone when the server runs and the backup posts to it.
pub fn server_status(listening: bool, bind: &str) -> String {
    if listening {
        format!("Il server di notifica risponde su {bind}.")
    } else {
        format!(
            "Il server di notifica non risponde su {bind}: perché le attività mandino la posta deve essere avviato (notify-server.exe --config <questo file>) e l'attività deve inviargli l'esito con --webhook-url http://{bind}/notify."
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fields() -> Fields {
        Fields {
            enabled: true,
            host: " smtp.example.com ".into(),
            port: "".into(),
            security: 0,
            username: "rustcopy@example.com".into(),
            password_name: "smtp-rustcopy".into(),
            from: "rustcopy@example.com".into(),
            to: "a@example.com; b@example.com,\n c@example.com ".into(),
            ..Fields::default()
        }
    }

    #[test]
    fn recipients_are_split_on_every_separator_without_blanks() {
        assert_eq!(
            recipients("a@x.it, b@x.it;c@x.it\n\n d@x.it ,"),
            vec!["a@x.it", "b@x.it", "c@x.it", "d@x.it"]
        );
        assert!(recipients("  ").is_empty());
    }

    #[test]
    fn the_screen_becomes_settings_with_the_password_as_a_name_only() {
        let s = settings(&fields()).expect("settings");
        assert_eq!(s.host, "smtp.example.com");
        assert_eq!(s.port, None);
        assert_eq!(s.security, "starttls");
        assert_eq!(s.password_spec, "keyring:smtp-rustcopy");
        assert_eq!(s.to.len(), 3);
        assert_eq!(s.problem(), None);
    }

    #[test]
    fn a_port_must_be_a_number_and_a_bad_one_is_a_sentence() {
        let mut f = fields();
        f.port = "587".into();
        assert_eq!(settings(&f).expect("settings").port, Some(587));
        f.port = "abc".into();
        assert!(settings(&f).expect_err("not a number").contains("porta"));
        f.port = "70000".into();
        assert!(settings(&f).is_err(), "out of range");
    }

    #[test]
    fn each_protection_has_its_place_in_the_choice() {
        for (index, name) in SECURITIES.iter().enumerate() {
            let mut f = fields();
            f.security = index as i32;
            assert_eq!(settings(&f).expect("settings").security, *name);
        }
    }

    /// What a file holds shows on the screen and comes back the same: the round trip loses nothing.
    #[test]
    fn a_file_round_trips_through_the_screen() {
        let original = settings(&fields()).expect("settings");
        let back = settings(&fields_from(&original)).expect("settings");
        assert_eq!(back, original);
    }

    /// A password someone wrote by hand in another form is kept as it is and never replaced by a name.
    #[test]
    fn a_hand_written_password_spec_is_carried_through() {
        let mut s = settings(&fields()).expect("settings");
        s.password_spec = "env:SMTP_PASSWORD".into();
        let screen = fields_from(&s);
        assert_eq!(screen.password_other, "env:SMTP_PASSWORD");
        assert_eq!(screen.password_name, "");
        assert_eq!(
            settings(&screen).expect("settings").password_spec,
            "env:SMTP_PASSWORD"
        );
    }

    #[test]
    fn the_answer_of_the_test_is_a_sentence_for_each_way_it_can_end() {
        let (ok, text) = test_outcome(0, "log\nMessaggio di prova inviato a admin@example.com.\n");
        assert!(ok && text.contains("admin@example.com"), "{text}");
        let (ok, text) = test_outcome(1, "Invio non riuscito: rifiutato\n");
        assert!(!ok && text.starts_with("Invio non riuscito"), "{text}");
        let (ok, text) = test_outcome(2, "");
        assert!(!ok && text.contains("non sono utilizzabili"), "{text}");
        let (ok, text) = test_outcome(1, "");
        assert!(!ok && text.contains("non risponde"), "{text}");
        let (ok, text) = test_outcome(77, "boh");
        assert!(!ok && text.contains("77"), "{text}");
    }

    #[test]
    fn the_server_status_says_what_is_missing() {
        assert!(server_status(true, "127.0.0.1:3000").contains("risponde"));
        let off = server_status(false, "127.0.0.1:3000");
        assert!(
            off.contains("non risponde") && off.contains("--webhook-url"),
            "{off}"
        );
    }
}
