use clap::Parser;
use color_eyre::eyre::Result;
use pmanager::cli::{Cli, Command};
use pmanager::{client, daemon};

#[tokio::main]
async fn main() -> Result<()> {
    color_eyre::install()?;

    let cli = Cli::parse();

    match cli.command.unwrap_or(Command::Tui) {
        Command::Tui => client::tui::run().await,
        Command::Daemon => daemon::run().await,
        Command::List => client::list_tunnels().await,
    }
}
