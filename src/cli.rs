use std::ffi::OsString;

use clap::{Args, Parser, Subcommand};

#[derive(Debug, Parser)]
#[command(
    name = "yd",
    version,
    about = "A personal terminal multitool",
    long_about = "yd is a personal terminal multitool. Wallet is the first module.",
    override_usage = "yd [OPTION]",
    after_help = "Wallet:\n  -w, --wallet  Show balances and wallet controls\n\nUse `yd -w -h` for wallet options.",
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
    /// Remove the locally stored wallet after confirmation
    #[arg(short, long)]
    pub reset: bool,

    /// Skip the reset confirmation prompt
    #[arg(short = 'y', long, requires = "reset")]
    pub yes: bool,
}

pub fn arguments() -> Vec<OsString> {
    let mut arguments = std::env::args_os().collect::<Vec<_>>();
    if let Some(position) = arguments
        .iter()
        .position(|argument| argument == "-w" || argument == "--wallet")
    {
        arguments[position] = OsString::from("wallet");
    }
    arguments
}
