use clap::{Args, Parser};

#[derive(Debug, Parser)]
#[command(
    name = "yd",
    version,
    about = "A personal terminal multitool",
    long_about = "yd is a personal terminal multitool. Wallet is the first module.",
    color = clap::ColorChoice::Auto
)]
pub struct Cli {
    #[command(flatten)]
    pub wallet: WalletArgs,
}

#[derive(Debug, Args)]
pub struct WalletArgs {
    /// Show Ethereum, Bitcoin, and Litecoin balances
    #[arg(short, long)]
    pub wallet: bool,

    /// Remove the locally stored wallet after confirmation
    #[arg(short, long, requires = "wallet")]
    pub reset: bool,

    /// Skip the reset confirmation prompt
    #[arg(short = 'y', long, requires = "reset")]
    pub yes: bool,
}
