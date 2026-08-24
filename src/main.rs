mod cli;
mod client;
mod daemon;
mod ipc;

use clap::Parser;
use cli::{Cli, Command};
use color_eyre::eyre::Result;

#[tokio::main]
async fn main() -> Result<()> {
    color_eyre::install()?;

    let cli = Cli::parse();

    match cli.command.unwrap_or(Command::Tui) {
        Command::Tui => client::tui::run().await,
        Command::Daemon => daemon::run().await,
    }
}
