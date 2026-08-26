use clap::Parser;
use color_eyre::eyre::Result;
use pmanager::cli::{Cli, Command, ServiceAction};
use pmanager::{client, daemon, service};

#[tokio::main]
async fn main() -> Result<()> {
    color_eyre::install()?;

    let cli = Cli::parse();

    match cli.command.unwrap_or(Command::Tui) {
        Command::Tui => client::tui::run().await,
        Command::Daemon => {
            pmanager::logging::init_daemon_logging(cli.log_level.as_deref());
            daemon::run().await
        }
        Command::List => client::list_tunnels().await,
        Command::Start { name } => client::start_tunnel(&name).await,
        Command::Stop { name } => client::stop_tunnel(&name).await,
        Command::Shutdown => client::shutdown_daemon().await,
        Command::Service { action } => match action {
            ServiceAction::Install => service::install(cli.log_level.as_deref()),
            ServiceAction::Uninstall => service::uninstall(),
        },
    }
}
