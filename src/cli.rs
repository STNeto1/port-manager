use clap::{Parser, Subcommand};

#[derive(Debug, Parser)]
#[command(
    name = "pmanager",
    version,
    about = "Manage SSH port-forwarding tunnels"
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Command>,

    /// Log level for the daemon (e.g. "info", "debug", "pmanager=trace").
    /// Overrides RUST_LOG. Has no effect on the TUI, which doesn't log.
    #[arg(long, global = true)]
    pub log_level: Option<String>,
}

#[derive(Debug, Clone, Subcommand)]
pub enum Command {
    /// Launch the TUI client (default)
    Tui,
    /// Run the daemon in the foreground
    Daemon,
    /// List configured tunnels and their status
    List,
    /// Start a tunnel by name; it keeps running in the daemon afterward
    Start { name: String },
    /// Stop a running tunnel by name
    Stop { name: String },
    /// Stop the daemon and all of its tunnels
    Shutdown,
}
