//! The mail settings the console edits (PIANO_GUI_ATTIVITA.md 3.5): the `[smtp]` section of the console's own
//! `notify-server.toml`.
//!
//! Sending mail is `notify-server`'s job, never the console's or the command line's: a backup posts its
//! summary to the server, and the server mails it. This module only reads and writes the section that tells
//! the server how, so the console needs no mail library of its own (`lettre` stays behind the `smtp` feature
//! of `notify-server`).
//!
//! Two rules keep it from being a way to leak a password:
//!
//! * the file holds a **secret specification**, never the secret: `keyring:NAME` for what the console saves
//!   (the password goes to Windows' Credential Manager under that name), or whatever `env:`/`file:` form
//!   someone wrote by hand, which is carried through untouched;
//! * a connection **without** encryption is accepted only to a loopback host, the same rule the sink
//!   enforces when it is built ([`crate::notify_sink::is_loopback`] is the one definition).
//!
//! Everything else in the file (the other channels, the bind address) is preserved when this section is
//! rewritten.

use std::path::{Path, PathBuf};

use crate::errors::IngestError;

const FILE_NAME: &str = "notify-server.toml";
const DEFAULT_BIND: &str = "127.0.0.1:3000";

/// What the page edits.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MailSettings {
    pub enabled: bool,
    pub host: String,
    pub port: Option<u16>,
    /// `starttls`, `tls` or `none`.
    pub security: String,
    pub username: String,
    /// The full secret specification as the file holds it (`keyring:NAME`, or an `env:`/`file:` form written
    /// by hand); empty when the server needs no password.
    pub password_spec: String,
    pub from: String,
    pub to: Vec<String>,
    pub subject_prefix: String,
}

impl Default for MailSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            host: String::new(),
            port: None,
            security: "starttls".to_string(),
            username: String::new(),
            password_spec: String::new(),
            from: String::new(),
            to: Vec::new(),
            subject_prefix: String::new(),
        }
    }
}

/// The console's own `notify-server.toml`, in its data folder.
pub fn default_path() -> PathBuf {
    crate::sessions::data_dir().join(FILE_NAME)
}

/// The name of the credential when the spec is `keyring:NAME`, else `None`.
pub fn keyring_name(spec: &str) -> Option<&str> {
    spec.trim()
        .strip_prefix("keyring:")
        .map(str::trim)
        .filter(|name| !name.is_empty())
}

fn looks_like_an_address(text: &str) -> bool {
    let text = text.trim();
    match text.split_once('@') {
        Some((user, host)) => {
            !user.is_empty()
                && host.contains('.')
                && !host.starts_with('.')
                && !host.ends_with('.')
                && !text.contains(char::is_whitespace)
        }
        None => false,
    }
}

impl MailSettings {
    /// The sentence that says what is wrong, or `None` when the settings can be saved.
    pub fn problem(&self) -> Option<String> {
        if self.host.trim().is_empty() {
            return Some("Scrivi il server di posta (per esempio smtp.example.com).".to_string());
        }
        if !matches!(self.security.as_str(), "starttls" | "tls" | "none") {
            return Some("La protezione deve essere starttls, tls o nessuna.".to_string());
        }
        if self.security == "none" && !crate::notify_sink::is_loopback(&self.host) {
            return Some(
                "Senza cifratura si può usare solo un server su questo computer (localhost): verso un altro server la password attraverserebbe la rete in chiaro."
                    .to_string(),
            );
        }
        if self.port == Some(0) {
            return Some("La porta deve essere un numero da 1 a 65535.".to_string());
        }
        if !looks_like_an_address(&self.from) {
            return Some(
                "Scrivi l'indirizzo del mittente (per esempio rustcopy@example.com).".to_string(),
            );
        }
        if self.to.is_empty() {
            return Some("Scrivi almeno un destinatario.".to_string());
        }
        if let Some(bad) = self.to.iter().find(|a| !looks_like_an_address(a)) {
            return Some(format!("«{bad}» non sembra un indirizzo di posta."));
        }
        if !self.username.trim().is_empty() && self.password_spec.trim().is_empty() {
            return Some("Con un nome utente serve anche la password.".to_string());
        }
        // A plain value in the password spot is the thing this file must never hold.
        let spec = self.password_spec.trim();
        if !spec.is_empty()
            && !["keyring:", "env:", "file:"]
                .iter()
                .any(|prefix| spec.starts_with(prefix))
        {
            return Some(
                "La password non si scrive nel file: salvala con «Chiavi e password» e usa il suo nome."
                    .to_string(),
            );
        }
        None
    }

    /// The `[smtp]` table these settings describe.
    fn to_table(&self) -> toml::Table {
        let mut table = toml::Table::new();
        table.insert("enabled".into(), self.enabled.into());
        table.insert("host".into(), self.host.trim().into());
        if let Some(port) = self.port {
            table.insert("port".into(), i64::from(port).into());
        }
        table.insert("security".into(), self.security.clone().into());
        if !self.username.trim().is_empty() {
            table.insert("username".into(), self.username.trim().into());
        }
        if !self.password_spec.trim().is_empty() {
            table.insert("password".into(), self.password_spec.trim().into());
        }
        table.insert("from".into(), self.from.trim().into());
        table.insert(
            "to".into(),
            toml::Value::Array(self.to.iter().map(|a| a.trim().into()).collect()),
        );
        if !self.subject_prefix.trim().is_empty() {
            table.insert("subject_prefix".into(), self.subject_prefix.trim().into());
        }
        table
    }

    fn from_table(table: &toml::Table) -> Self {
        let text = |key: &str| {
            table
                .get(key)
                .and_then(toml::Value::as_str)
                .unwrap_or_default()
                .to_string()
        };
        Self {
            enabled: table
                .get("enabled")
                .and_then(toml::Value::as_bool)
                .unwrap_or(false),
            host: text("host"),
            port: table
                .get("port")
                .and_then(toml::Value::as_integer)
                .and_then(|p| u16::try_from(p).ok()),
            security: {
                let value = text("security");
                if value.is_empty() {
                    "starttls".to_string()
                } else {
                    value
                }
            },
            username: text("username"),
            password_spec: text("password"),
            from: text("from"),
            to: table
                .get("to")
                .and_then(toml::Value::as_array)
                .map(|items| {
                    items
                        .iter()
                        .filter_map(toml::Value::as_str)
                        .map(str::to_string)
                        .collect()
                })
                .unwrap_or_default(),
            subject_prefix: text("subject_prefix"),
        }
    }
}

fn read_table(path: &Path) -> toml::Table {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|text| text.parse::<toml::Table>().ok())
        .unwrap_or_default()
}

/// The settings the file holds; defaults (switched off) when the file or its `[smtp]` section is missing or
/// unreadable.
pub fn read(path: &Path) -> MailSettings {
    read_table(path)
        .get("smtp")
        .and_then(toml::Value::as_table)
        .map(MailSettings::from_table)
        .unwrap_or_default()
}

/// Writes `settings` as the `[smtp]` section of `path`, keeping every other section and key. Refuses
/// settings that [`MailSettings::problem`] objects to; the file is replaced atomically.
pub fn write(path: &Path, settings: &MailSettings) -> Result<(), IngestError> {
    if let Some(problem) = settings.problem() {
        return Err(IngestError::CopyPlanInvalid(problem));
    }
    let mut table = read_table(path);
    table.insert("smtp".into(), toml::Value::Table(settings.to_table()));
    let body = toml::to_string_pretty(&table).map_err(|error| {
        IngestError::io(
            path,
            std::io::Error::new(std::io::ErrorKind::InvalidData, error),
        )
    })?;
    let text = format!(
        "# Scritto dalla console di rustcopy (pagina «Notifiche per posta»). Le altre sezioni si conservano.\n{body}"
    );
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| IngestError::io(dir, e))?;
    }
    crate::atomic_write(path, text.as_bytes()).map_err(|e| IngestError::io(path, e))
}

/// The address the server listens on, as the file says (`bind`), else the default.
pub fn bind_address(path: &Path) -> String {
    read_table(path)
        .get("bind")
        .and_then(toml::Value::as_str)
        .map(str::to_string)
        .unwrap_or_else(|| DEFAULT_BIND.to_string())
}

/// Whether something accepts connections at `address` (`host:port`) right now. A plain connect with a short
/// timeout: it says the server process is there, not that it is healthy.
pub fn server_is_listening(address: &str) -> bool {
    use std::net::ToSocketAddrs;
    address
        .to_socket_addrs()
        .ok()
        .and_then(|mut addresses| addresses.next())
        .is_some_and(|addr| {
            std::net::TcpStream::connect_timeout(&addr, std::time::Duration::from_millis(400))
                .is_ok()
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn good() -> MailSettings {
        MailSettings {
            enabled: true,
            host: "smtp.example.com".into(),
            port: Some(587),
            security: "starttls".into(),
            username: "rustcopy@example.com".into(),
            password_spec: "keyring:smtp-rustcopy".into(),
            from: "rustcopy@example.com".into(),
            to: vec!["admin@example.com".into()],
            subject_prefix: String::new(),
        }
    }

    #[test]
    fn good_settings_have_no_problem() {
        assert_eq!(good().problem(), None);
    }

    #[test]
    fn each_thing_that_is_missing_or_wrong_is_a_sentence() {
        let mut s = good();
        s.host.clear();
        assert!(s.problem().expect("host").contains("server"));
        let mut s = good();
        s.from = "senza-chiocciola".into();
        assert!(s.problem().expect("from").contains("mittente"));
        let mut s = good();
        s.to.clear();
        assert!(s.problem().expect("to").contains("destinatario"));
        let mut s = good();
        s.to = vec!["a@b.it".into(), "rotto".into()];
        assert!(s.problem().expect("bad to").contains("«rotto»"));
        let mut s = good();
        s.security = "boh".into();
        assert!(s.problem().expect("security").contains("protezione"));
        let mut s = good();
        s.port = Some(0);
        assert!(s.problem().expect("port").contains("porta"));
        let mut s = good();
        s.password_spec.clear();
        assert!(s.problem().expect("password").contains("password"));
    }

    /// The rule the sink enforces when it is built, said earlier: no encryption only to this computer.
    #[test]
    fn no_encryption_is_only_for_the_local_computer() {
        let mut s = good();
        s.security = "none".into();
        assert!(s.problem().expect("remote").contains("localhost"));
        s.host = "localhost".into();
        s.username.clear();
        s.password_spec.clear();
        assert_eq!(s.problem(), None);
        s.host = "127.0.0.1".into();
        assert_eq!(s.problem(), None);
    }

    /// The one thing this file must never hold: a password written out.
    #[test]
    fn a_plain_password_in_the_file_is_refused() {
        let mut s = good();
        s.password_spec = "la-mia-password".into();
        assert!(s.problem().expect("plain").contains("Chiavi e password"));
        for spec in [
            "keyring:nome",
            "env:SMTP_PASSWORD",
            "file:C:/segreti/smtp.txt",
        ] {
            s.password_spec = spec.into();
            assert_eq!(s.problem(), None, "{spec}");
        }
        assert_eq!(keyring_name("keyring:smtp-rustcopy"), Some("smtp-rustcopy"));
        assert_eq!(keyring_name("env:X"), None);
        assert_eq!(keyring_name("keyring:"), None);
    }

    #[test]
    fn settings_round_trip_through_the_file() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("notify-server.toml");
        let mut s = good();
        s.subject_prefix = "[backup]".into();
        write(&path, &s).expect("write");
        assert_eq!(read(&path), s);
        let text = std::fs::read_to_string(&path).expect("read");
        assert!(text.contains("keyring:smtp-rustcopy"));
        assert!(!text.contains("password = \"la-mia"), "no secret value");
    }

    /// Rewriting the mail section keeps every other section and key, and the file's own address.
    #[test]
    fn the_other_sections_survive_a_rewrite() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("notify-server.toml");
        std::fs::write(
            &path,
            "bind = \"127.0.0.1:4000\"\n\n[ntfy]\nenabled = true\ntopic_url = \"https://ntfy.sh/x\"\n\n[smtp]\nenabled = false\nhost = \"vecchio.example.com\"\nfrom = \"a@b.it\"\nto = [\"c@d.it\"]\n",
        )
        .expect("seed");
        assert_eq!(read(&path).host, "vecchio.example.com");
        write(&path, &good()).expect("write");
        let table = std::fs::read_to_string(&path)
            .expect("read")
            .parse::<toml::Table>()
            .expect("parses");
        assert_eq!(table["bind"].as_str(), Some("127.0.0.1:4000"));
        assert_eq!(
            table["ntfy"]["topic_url"].as_str(),
            Some("https://ntfy.sh/x")
        );
        assert_eq!(table["smtp"]["host"].as_str(), Some("smtp.example.com"));
        assert_eq!(bind_address(&path), "127.0.0.1:4000");
    }

    #[test]
    fn bad_settings_are_refused_before_anything_is_written() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("notify-server.toml");
        let mut s = good();
        s.password_spec = "in chiaro".into();
        assert!(write(&path, &s).is_err());
        assert!(!path.exists(), "nothing written for a refused setting");
    }

    #[test]
    fn a_missing_or_broken_file_reads_as_switched_off_defaults() {
        let dir = tempfile::tempdir().expect("tempdir");
        assert_eq!(read(&dir.path().join("nope.toml")), MailSettings::default());
        let broken = dir.path().join("broken.toml");
        std::fs::write(&broken, "questo [non è toml").expect("write");
        assert_eq!(read(&broken), MailSettings::default());
        assert_eq!(bind_address(&broken), "127.0.0.1:3000");
        assert!(!MailSettings::default().enabled);
    }

    #[test]
    fn a_listening_port_is_seen_and_a_closed_one_is_not() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind");
        let address = listener.local_addr().expect("addr").to_string();
        assert!(server_is_listening(&address));
        drop(listener);
        assert!(!server_is_listening(&address));
        assert!(!server_is_listening("non un indirizzo"));
    }
}
