//! `notify-server`: standalone receiver for `robocopy_ingest --webhook-url` notifications.
//!
//! Thin binary: all the real logic (router, handlers, security checks) lives in
//! `robocopy_ingest::notify_server` and `robocopy_ingest::notify_sink` so integration tests can
//! exercise it directly over a real socket. This file only does argument parsing, config loading,
//! and process wiring (tracing init, bind, serve, graceful shutdown).
//!
//! Only built with `--features notify-server` (see the `[[bin]]` / `required-features` entry in
//! Cargo.toml) — the default backup binary (`robocopy_ingest`) never links axum.
//!
//! **F41**: can also install/run itself as its own Windows service (`"RustcopyNotifyServer"`),
//! separate from `robocopy_ingest`'s own idle service (F37, `"RustcopyIngestService"`) — see
//! `src/service.rs`'s doc comment for why these are two distinct identities rather than one
//! binary hosting the other's work. `main()` is a plain `fn`, not `#[tokio::main]`, for the same
//! reason `robocopy_ingest::main()` is: `service_dispatcher::start` blocks the calling OS thread
//! until SCM stops the service and must not run on a tokio runtime worker thread, so the
//! service-launch check has to happen *before* any tokio `Runtime` exists.

use std::net::SocketAddr;
use std::path::PathBuf;
use std::time::Duration;

use anyhow::{Context, Result};
use clap::Parser;

use robocopy_ingest::notify_server::{
    build_router, check_bind_security, serve_until_shutdown_or, AppState,
};
use robocopy_ingest::notify_sink::NotifyServerConfig;

/// Environment variable holding the bearer token required on `/notify` (unset = no auth, only
/// safe on a loopback bind — see `check_bind_security`).
const TOKEN_ENV_VAR: &str = "ROBOCOPY_NOTIFY_TOKEN";
/// Environment variable overriding the bind address when `--bind` isn't passed.
const BIND_ENV_VAR: &str = "ROBOCOPY_NOTIFY_BIND";
const DEFAULT_BIND: &str = "127.0.0.1:3000";
/// Timeout for outbound channel deliveries (ntfy, generic webhook).
const SINK_TIMEOUT: Duration = Duration::from_secs(10);

/// F41: this binary's own Windows service identity — distinct from `robocopy_ingest`'s
/// `"RustcopyIngestService"` (F37). See this file's and `service.rs`'s doc comments for why.
const SERVICE_NAME: &str = "RustcopyNotifyServer";
const SERVICE_DISPLAY_NAME: &str = "Rustcopy Notify Server";

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

    /// F41: install this exact invocation (minus the service flags themselves) as a Windows
    /// service ("RustcopyNotifyServer") and exit — the service then runs the server persistently,
    /// restartable/stoppable via `services.msc`/`sc`. Requires Administrator.
    #[arg(long, default_value_t = false, conflicts_with = "uninstall_service")]
    install_service: bool,

    /// F41: remove the previously installed service and exit. Requires Administrator.
    #[arg(long, default_value_t = false, conflicts_with = "install_service")]
    uninstall_service: bool,
}

fn main() -> Result<()> {
    if robocopy_ingest::service::is_service_launch() {
        return run_as_service();
    }

    tracing_subscriber::fmt::init();
    let args = Args::parse();

    if args.install_service {
        return install_service();
    }
    if args.uninstall_service {
        robocopy_ingest::service::uninstall_named(SERVICE_NAME)
            .context("cannot uninstall the Windows service")?;
        println!("Windows service '{SERVICE_NAME}' removed.");
        return Ok(());
    }

    let runtime =
        tokio::runtime::Runtime::new().context("failed to build the tokio async runtime")?;
    // No extra shutdown source beyond Ctrl+C/SIGTERM in the normal foreground run — `pending()`
    // is a future that never resolves, so `serve_until_shutdown_or` behaves exactly like the
    // pre-F41 `serve_until_shutdown` here.
    runtime.block_on(run_server(args, std::future::pending()))
}

/// F41: registers this binary's own invocation (captured from the real argv, minus the
/// `--install-service`/`--uninstall-service` flags themselves — same "use the real argv, not a
/// reconstruction" discipline as `schedule::strip_schedule_flags`, F36) as a Windows service.
fn install_service() -> Result<()> {
    let launch_arguments: Vec<std::ffi::OsString> = std::env::args()
        .skip(1)
        .filter(|arg| arg != "--install-service" && arg != "--uninstall-service")
        .map(std::ffi::OsString::from)
        .collect();

    robocopy_ingest::service::install_named(SERVICE_NAME, SERVICE_DISPLAY_NAME, launch_arguments)
        .context("cannot install the Windows service")?;
    println!(
        "Windows service '{SERVICE_NAME}' installed (start type: OnDemand). Start it with:\n  sc start {SERVICE_NAME}\nor via services.msc."
    );
    Ok(())
}

/// The shared server body: loads config, resolves the bind address, checks bind security, builds
/// the router, and serves until `extra_shutdown` resolves (in addition to the always-present
/// Ctrl+C/SIGTERM handling inside `serve_until_shutdown_or`). Used by both the normal foreground
/// run (`extra_shutdown` = a future that never resolves) and the F41 service body (`extra_shutdown`
/// = a bridge from SCM's `Stop` control event).
async fn run_server(
    args: Args,
    extra_shutdown: impl std::future::Future<Output = ()> + Send + 'static,
) -> Result<()> {
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
    serve_until_shutdown_or(listener, router, extra_shutdown)
        .await
        .context("server error")?;

    tracing::info!("notify-server shut down cleanly");
    Ok(())
}

#[cfg(windows)]
windows_service::define_windows_service!(ffi_service_main, run_notify_service_body);

#[cfg(windows)]
fn run_notify_service_body(_arguments: Vec<std::ffi::OsString>) {
    if let Err(error) = run_notify_service_inner() {
        eprintln!("error: {error:#}");
    }
}

/// F41's actual service body. Rebuilds `Args` from the real process argv (minus the internal
/// `RUN_AS_SERVICE_ARG` marker) rather than trusting SCM's `arguments` callback parameter — same
/// "the real argv is the source of truth" approach `service::is_service_launch()` already uses,
/// so `--bind`/`--config` given alongside `--install-service` reach the running service exactly
/// as typed. Bridges the synchronous SCM `Stop` signal (`register_and_wait_for_stop` returns a
/// blocking `mpsc::Receiver`) into the async world via a `spawn_blocking` task feeding a
/// `tokio::sync::oneshot` that `run_server`'s `extra_shutdown` future awaits — `axum::serve`'s
/// graceful shutdown only understands async signals, not a blocking channel.
#[cfg(windows)]
fn run_notify_service_inner() -> Result<()> {
    let raw_args: Vec<String> = std::env::args()
        .skip(1)
        .filter(|arg| arg != robocopy_ingest::service::RUN_AS_SERVICE_ARG)
        .collect();
    let args = Args::parse_from(std::iter::once("notify-server".to_string()).chain(raw_args));

    let (status_handle, stop_rx) =
        robocopy_ingest::service::register_and_wait_for_stop(SERVICE_NAME)?;

    let (async_stop_tx, async_stop_rx) = tokio::sync::oneshot::channel::<()>();
    let runtime =
        tokio::runtime::Runtime::new().context("failed to build the tokio async runtime")?;
    runtime.spawn_blocking(move || {
        let _ = stop_rx.recv();
        let _ = async_stop_tx.send(());
    });

    let result = runtime.block_on(run_server(args, async move {
        let _ = async_stop_rx.await;
    }));

    // `run_server` can return for its own reasons -- a bind failure, most plainly -- and not
    // because SCM asked us to stop. In that case the `spawn_blocking` task above is still parked
    // in `stop_rx.recv()`, which only returns on a Stop that will never come. Dropping the runtime
    // waits for blocking tasks to finish, so the process would report `Stopped` to SCM and then
    // hang forever: a service that looks stopped, holds its port, and cannot be restarted.
    //
    // A zero timeout detaches that task instead of waiting for it. Nothing is lost: its only job
    // was to forward a stop signal we are no longer waiting for.
    runtime.shutdown_timeout(std::time::Duration::from_secs(0));

    status_handle.report_stopped();
    result
}

#[cfg(windows)]
fn run_as_service() -> Result<()> {
    robocopy_ingest::service::start_dispatcher(SERVICE_NAME, ffi_service_main).map_err(Into::into)
}

#[cfg(not(windows))]
fn run_as_service() -> Result<()> {
    anyhow::bail!(
        "service mode requires the Windows Service Control Manager, unavailable on this platform"
    )
}
