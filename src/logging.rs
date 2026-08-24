use tracing_subscriber::EnvFilter;

/// Installs a tracing subscriber writing to stdout. Only called for the
/// daemon: when auto-spawned, its stdout is already redirected to
/// `daemon.log` (see `client::spawn_daemon`), and when run manually via
/// `pmanager daemon` under a supervisor, stdout is the standard place a
/// long-running process is expected to log to. The TUI never calls this —
/// its stdout is the terminal UI itself (raw mode/alt screen), so writing
/// log lines there would corrupt the display.
pub fn init_daemon_logging(log_level: Option<&str>) {
    let filter = match log_level {
        Some(level) => EnvFilter::new(level),
        None => EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
    };

    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(false)
        .init();
}
