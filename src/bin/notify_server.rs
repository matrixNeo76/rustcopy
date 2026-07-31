//! `notify-server`: standalone receiver for `robocopy_ingest --webhook-url` notifications.
//!
//! Thin binary: all the real logic (router, handlers, security checks) lives in
//! `robocopy_ingest::notify_server` and `robocopy_ingest::notify_sink` so integration tests can
//! exercise it directly over a real socket. This file only does argument parsing, config loading,
//! and process wiring (tracing init, bind, serve, graceful shutdown).
//!
//! Only built with `--features notify-server` (see the `[[bin]]` / `required-features` entry in
//! Cargo.toml) — the default backup binary (`robocopy_ingest`) never links axum.

use std::net::SocketAddr;
use std::path::PathBuf;
use std::time::Duration;

use anyhow::{Context, Result};
use clap::Parser;

use robocopy_ingest::notify_server::{build_router, check_bind_security, serve_until_shutdown, AppState};
use robocopy_ingest::notify_sink::NotifyServerConfig;

/// Environment variable holding the bearer token required on `/notify` (unset = no auth, only
/// safe on a loopback bind — see `check_bind_security`).
const TOKEN_ENV_VAR: &str = "ROBOCOPY_NOTIFY_TOKEN";
/// Environment variable overriding the bind address when `--bind` isn't passed.
const BIND_ENV_VAR: &str = "ROBOCOPY_NOTIFY_BIND";
const DEFAULT_BIND: &str = "127.0.0.1:3000";
/// Timeout for outbound channel deliveries (ntfy, generic webhook).
const SINK_TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Parser, Debug)]
#[command(
    name = "notify-server",
    version,
    about = "Receives robocopy_ingest --webhook-url notifications and forwards them to configured channels (ntfy, generic webhook, log)."
)]
struct Args {
    /// Address to bind to. Overrides ROBOCOPY_NOTIFY_BIND and the config file's `bind`.
    #[arg(long, value_name = "HOST:PORT")]
    bind: Option<String>,

    /// Path to a TOML config file (see PIANO_NOTIFY_SERVER.md / README for the schema).
    #[arg(long, value_name = "PATH")]
    config: Option<PathBuf>,
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();

    let args = Args::parse();

    let config = match &args.config {
        Some(path) => NotifyServerConfig::load_from(path)
            .with_context(|| format!("cannot load config file from {}", path.display()))?,
        None => NotifyServerConfig::default(),
    };

    let bind_str = args
        .bind
        .or_else(|| std::env::var(BIND_ENV_VAR).ok())
        .or_else(|| config.bind.clone())
        .unwrap_or_else(|| DEFAULT_BIND.to_string());
    let addr: SocketAddr = bind_str
        .parse()
        .with_context(|| format!("invalid bind address: {bind_str}"))?;

    let token = std::env::var(TOKEN_ENV_VAR).ok().filter(|t| !t.is_empty());

    check_bind_security(&addr, &token).map_err(anyhow::Error::msg)?;
    if token.is_none() {
        tracing::warn!(
            "no {TOKEN_ENV_VAR} configured: /notify accepts unauthenticated requests (safe only \
             because the bind address is loopback)"
        );
    }

    let sinks = config.build_sinks(SINK_TIMEOUT);
    tracing::info!(
        channels = ?sinks.iter().map(|s| s.name()).collect::<Vec<_>>(),
        "notify-server starting"
    );

    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .with_context(|| format!("cannot bind {addr} (already in use?)"))?;
    let actual_addr = listener.local_addr().context("cannot read bound address")?;
    // Deliberately a plain println!, not a tracing line: operators and tests alike need one
    // predictable, unformatted line to find the real port when binding to :0.
    println!("robocopy-ingest notify-server listening on {actual_addr}");
    tracing::info!(address = %actual_addr, "listening");

    let router = build_router(AppState { token, sinks });
    serve_until_shutdown(listener, router)
        .await
        .context("server error")?;

    tracing::info!("notify-server shut down cleanly");
    Ok(())
}
