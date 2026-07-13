use color_eyre::eyre::Result;
use owo_colors::OwoColorize;

use crate::{cli::Cli, wallet::WalletService};

pub struct Application {
    cli: Cli,
}

impl Application {
    pub fn new(cli: Cli) -> Self {
        Self { cli }
    }

    pub async fn run(self) -> Result<()> {
        if self.cli.wallet {
            WalletService::open()?.show_portfolio().await?;
        } else {
            println!(
                "{} Run {} to view your portfolio.",
                "yd".bold().cyan(),
                "yd --wallet".bold()
            );
        }
        Ok(())
    }
}
