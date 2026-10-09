//! The email channel of `notify-server` (GUI Slint plan, phase 6): a [`SmtpSink`] that mails the same
//! summary the other channels send. Compiled only with the `smtp` feature, which `notify-server` turns on:
//! the command line and the console never send mail themselves, they post to `notify-server`.
//!
//! The password is never a plain value in a file: it is a [`crate::crypto::resolve_key`] spec
//! (`keyring:NAME`, `env:NAME`, `file:PATH`), read when a message is sent. A connection **without**
//! encryption is accepted only to a loopback host (a local relay or a test server); anywhere else the sink
//! refuses to be built, so a password cannot cross a network in the clear by a configuration slip.

use std::time::Duration;

use async_trait::async_trait;
use lettre::message::{header::ContentType, Mailbox};
use lettre::transport::smtp::authentication::Credentials;
use lettre::{AsyncSmtpTransport, AsyncTransport, Message, Tokio1Executor};

use crate::notify::{BackupStatus, WebhookPayload};
use crate::notify_sink::{NotificationSink, NotifyError, SmtpChannelConfig};

const SINK: &str = "smtp";

/// How the connection to the mail server is protected.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Security {
    /// Plain connection upgraded with STARTTLS (usually port 587).
    StartTls,
    /// TLS from the first byte (usually port 465).
    Tls,
    /// No encryption: loopback hosts only.
    None,
}

impl Security {
    fn parse(text: Option<&str>) -> Result<Self, String> {
        match text.map(|t| t.trim().to_lowercase()).as_deref() {
            None | Some("") | Some("starttls") => Ok(Security::StartTls),
            Some("tls") | Some("ssl") => Ok(Security::Tls),
            Some("none") => Ok(Security::None),
            Some(other) => Err(format!(
                "security \"{other}\" is not one of starttls, tls, none"
            )),
        }
    }
}

fn is_loopback(host: &str) -> bool {
    matches!(
        host.trim().to_lowercase().as_str(),
        "localhost" | "127.0.0.1" | "::1" | "[::1]"
    )
}

/// Mails a backup notification.
pub struct SmtpSink {
    host: String,
    port: u16,
    security: Security,
    username: Option<String>,
    password_spec: Option<String>,
    from: Mailbox,
    to: Vec<Mailbox>,
    subject_prefix: String,
    timeout: Duration,
}

impl SmtpSink {
    /// Builds the sink from its configuration, refusing what cannot work or would be unsafe.
    pub fn new(config: &SmtpChannelConfig, timeout: Duration) -> Result<Self, String> {
        let security = Security::parse(config.security.as_deref())?;
        if security == Security::None && !is_loopback(&config.host) {
            return Err(format!(
                "security = \"none\" is allowed only for a loopback host, not \"{}\": a password would cross the network in the clear",
                config.host
            ));
        }
        let from: Mailbox = config
            .from
            .parse()
            .map_err(|e| format!("from \"{}\": {e}", config.from))?;
        if config.to.is_empty() {
            return Err("to is empty: there is nobody to mail".to_string());
        }
        let to = config
            .to
            .iter()
            .map(|address| {
                address
                    .parse::<Mailbox>()
                    .map_err(|e| format!("to \"{address}\": {e}"))
            })
            .collect::<Result<Vec<_>, _>>()?;
        let default_port = match security {
            Security::Tls => 465,
            Security::StartTls => 587,
            Security::None => 25,
        };
        Ok(Self {
            host: config.host.clone(),
            port: config.port.unwrap_or(default_port),
            security,
            username: config.username.clone(),
            password_spec: config.password.clone(),
            from,
            to,
            subject_prefix: config
                .subject_prefix
                .clone()
                .unwrap_or_else(|| "[rustcopy]".to_string()),
            timeout,
        })
    }

    fn subject(&self, payload: &WebhookPayload) -> String {
        let state = match payload.status {
            BackupStatus::Success => "OK",
            BackupStatus::Failed => "FALLITO",
        };
        format!(
            "{} {state} {}: {}",
            self.subject_prefix, payload.host, payload.source
        )
    }

    fn body(payload: &WebhookPayload) -> String {
        format!(
            "{}\n\nOrigine: {}\nDestinazione: {}\nComputer: {}\nFile copiati: {}\nByte copiati: {}\nDurata: {:.1} s\nVersione: {}\n{}{}",
            payload.text,
            payload.source,
            payload.dest,
            payload.host,
            payload.files_copied,
            payload.bytes_copied,
            payload.elapsed_seconds,
            payload.tool_version,
            payload
                .exit_code
                .map(|code| format!("Codice di uscita: {code}\n"))
                .unwrap_or_default(),
            payload
                .integrity_status
                .as_ref()
                .map(|status| format!("Verifica: {status}\n"))
                .unwrap_or_default(),
        )
    }

    fn message(&self, payload: &WebhookPayload) -> Result<Message, String> {
        let mut builder = Message::builder()
            .from(self.from.clone())
            .subject(self.subject(payload))
            .header(ContentType::TEXT_PLAIN);
        for recipient in &self.to {
            builder = builder.to(recipient.clone());
        }
        builder.body(Self::body(payload)).map_err(|e| e.to_string())
    }

    fn transport(
        &self,
        credentials: Option<Credentials>,
    ) -> Result<AsyncSmtpTransport<Tokio1Executor>, String> {
        let builder = match self.security {
            Security::StartTls => AsyncSmtpTransport::<Tokio1Executor>::starttls_relay(&self.host),
            Security::Tls => AsyncSmtpTransport::<Tokio1Executor>::relay(&self.host),
            Security::None => Ok(AsyncSmtpTransport::<Tokio1Executor>::builder_dangerous(
                &self.host,
            )),
        }
        .map_err(|e| e.to_string())?;
        let builder = builder.port(self.port).timeout(Some(self.timeout));
        Ok(match credentials {
            Some(credentials) => builder.credentials(credentials).build(),
            None => builder.build(),
        })
    }
}

#[async_trait]
impl NotificationSink for SmtpSink {
    fn name(&self) -> &'static str {
        SINK
    }

    async fn deliver(&self, payload: &WebhookPayload) -> Result<(), NotifyError> {
        let fail = |message: String| NotifyError {
            sink: SINK,
            message,
        };
        let message = self.message(payload).map_err(fail)?;

        // The password is read now, not at start-up, so a rotated credential is picked up; the lookup
        // may touch the Credential Manager, so it runs off the async threads.
        let credentials = match (&self.username, &self.password_spec) {
            (Some(user), Some(spec)) => {
                let spec = spec.clone();
                let password =
                    tokio::task::spawn_blocking(move || crate::crypto::resolve_key(&spec))
                        .await
                        .map_err(|e| fail(e.to_string()))?
                        .map_err(|e| fail(format!("cannot read the password: {e}")))?;
                Some(Credentials::new(user.clone(), password))
            }
            _ => None,
        };
        let transport = self.transport(credentials).map_err(fail)?;
        // The whole exchange is bounded: lettre's own timeout covers each command, this covers a server
        // that accepts the connection and then goes silent.
        tokio::time::timeout(self.timeout * 4, transport.send(message))
            .await
            .map_err(|_| fail("the mail server did not answer in time".to_string()))?
            .map_err(|e| fail(e.to_string()))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
    use tokio::net::TcpListener;

    fn config(host: &str, port: u16) -> SmtpChannelConfig {
        SmtpChannelConfig {
            enabled: true,
            host: host.to_string(),
            port: Some(port),
            security: Some("none".to_string()),
            username: None,
            password: None,
            from: "rustcopy@example.test".to_string(),
            to: vec![
                "admin@example.test".to_string(),
                "second@example.test".to_string(),
            ],
            subject_prefix: None,
        }
    }

    fn payload(status: BackupStatus) -> WebhookPayload {
        WebhookPayload {
            schema_version: crate::notify::NOTIFY_SCHEMA_VERSION,
            text: "backup finito".to_string(),
            report_summary: "3 file".to_string(),
            status,
            files_copied: 3,
            bytes_copied: 300,
            elapsed_seconds: 2.5,
            source: "D:/dati".to_string(),
            dest: "E:/copia".to_string(),
            host: "srv01".to_string(),
            tool_version: "7.8.1".to_string(),
            exit_code: Some(1),
            integrity_status: Some("PASSED".to_string()),
        }
    }

    /// A tiny SMTP server that accepts one message and returns what it received.
    async fn fake_server() -> (u16, tokio::task::JoinHandle<String>) {
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let port = listener.local_addr().expect("addr").port();
        let handle = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.expect("accept");
            let (read, mut write) = stream.into_split();
            let mut lines = BufReader::new(read).lines();
            write.write_all(b"220 fake ESMTP\r\n").await.expect("greet");
            let mut received = String::new();
            let mut in_data = false;
            while let Ok(Some(line)) = lines.next_line().await {
                if in_data {
                    if line == "." {
                        in_data = false;
                        write.write_all(b"250 queued\r\n").await.expect("queued");
                    } else {
                        received.push_str(&line);
                        received.push('\n');
                    }
                    continue;
                }
                let upper = line.to_uppercase();
                received.push_str(&format!("> {line}\n"));
                let reply: &[u8] = if upper.starts_with("EHLO") || upper.starts_with("HELO") {
                    b"250 fake\r\n"
                } else if upper.starts_with("DATA") {
                    in_data = true;
                    b"354 go ahead\r\n"
                } else if upper.starts_with("QUIT") {
                    write.write_all(b"221 bye\r\n").await.expect("bye");
                    break;
                } else {
                    b"250 ok\r\n"
                };
                write.write_all(reply).await.expect("reply");
            }
            received
        });
        (port, handle)
    }

    #[tokio::test]
    async fn a_notification_reaches_every_recipient_with_a_readable_subject_and_body() {
        let (port, server) = fake_server().await;
        let sink = SmtpSink::new(&config("127.0.0.1", port), Duration::from_secs(5)).expect("sink");
        sink.deliver(&payload(BackupStatus::Failed))
            .await
            .expect("delivered");

        let received = server.await.expect("server");
        assert!(
            received.contains("> MAIL FROM:<rustcopy@example.test>"),
            "{received}"
        );
        assert!(
            received.contains("RCPT TO:<admin@example.test>"),
            "{received}"
        );
        assert!(
            received.contains("RCPT TO:<second@example.test>"),
            "{received}"
        );
        assert!(
            received.contains("Subject: [rustcopy] FALLITO srv01: D:/dati"),
            "{received}"
        );
        assert!(received.contains("File copiati: 3"), "{received}");
        assert!(received.contains("Verifica: PASSED"), "{received}");
    }

    #[test]
    fn an_unencrypted_connection_is_refused_for_anything_but_a_loopback_host() {
        let mut remote = config("mail.example.com", 25);
        remote.security = Some("none".to_string());
        assert!(SmtpSink::new(&remote, Duration::from_secs(1)).is_err());
        assert!(SmtpSink::new(&config("localhost", 25), Duration::from_secs(1)).is_ok());
        remote.security = Some("starttls".to_string());
        assert!(SmtpSink::new(&remote, Duration::from_secs(1)).is_ok());
    }

    #[test]
    fn a_bad_address_or_an_empty_list_or_an_unknown_security_is_refused_with_the_reason() {
        let mut bad_from = config("localhost", 25);
        bad_from.from = "not an address".to_string();
        assert!(SmtpSink::new(&bad_from, Duration::from_secs(1)).is_err());
        let mut nobody = config("localhost", 25);
        nobody.to.clear();
        assert!(SmtpSink::new(&nobody, Duration::from_secs(1)).is_err());
        let mut odd = config("localhost", 25);
        odd.security = Some("quantum".to_string());
        assert!(SmtpSink::new(&odd, Duration::from_secs(1)).is_err());
    }

    #[test]
    fn the_default_ports_follow_the_security_mode() {
        let mut tls = config("mail.example.com", 0);
        tls.port = None;
        tls.security = Some("tls".to_string());
        assert_eq!(
            SmtpSink::new(&tls, Duration::from_secs(1))
                .expect("tls")
                .port,
            465
        );
        tls.security = None;
        assert_eq!(
            SmtpSink::new(&tls, Duration::from_secs(1))
                .expect("starttls")
                .port,
            587
        );
    }

    #[tokio::test]
    async fn a_dead_server_is_reported_as_a_failure_not_a_hang() {
        // Nothing listens on this port.
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let port = listener.local_addr().expect("addr").port();
        drop(listener);
        let sink = SmtpSink::new(&config("127.0.0.1", port), Duration::from_secs(1)).expect("sink");
        let error = sink
            .deliver(&payload(BackupStatus::Success))
            .await
            .expect_err("fails");
        assert_eq!(error.sink, "smtp");
    }
}
