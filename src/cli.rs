use clap::{Args, CommandFactory, FromArgMatches, Parser, Subcommand};

use crate::commands;

#[derive(Debug, Parser)]
#[command(
    name = "yd",
    version,
    about = "A personal terminal multitool",
    long_about = "yd is a personal terminal multitool. Wallet is the first module.",
    override_usage = "yd [OPTION]",
    color = clap::ColorChoice::Auto
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Command>,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Cryptocurrency wallet portfolio and controls
    #[command(hide = true)]
    Wallet(WalletArgs),
}

#[derive(Debug, Args)]
#[command(
    about = "Show balances and manage the local wallet",
    override_usage = "yd -w [OPTION]"
)]
pub struct WalletArgs {
    /// Show wallet derivation paths without fetching balances
    #[arg(short = 'p', long, conflicts_with = "reset")]
    pub paths: bool,

    /// Remove the locally stored wallet after confirmation
    #[arg(short, long)]
    pub reset: bool,

    /// Skip the reset confirmation prompt
    #[arg(short = 'y', long, requires = "reset")]
    pub yes: bool,
}

pub fn parse() -> Cli {
    let command = Cli::command().after_help(commands::root_help());
    let matches =
        command.get_matches_from(commands::normalize_arguments(std::env::args_os().collect()));
    Cli::from_arg_matches(&matches).expect("clap returned matches compatible with Cli")
}
