use crate::{
    cli::{Cli, Command, WalletAction},
    commands::MODULES,
    ui::{Tone, Ui},
    wallet::WalletService,
};
use color_eyre::eyre::Result;

pub struct Application {
    cli: Cli,
}

impl Application {
    pub fn new(cli: Cli) -> Self {
        Self { cli }
    }

    pub async fn run(self) -> Result<()> {
        match self.cli.command {
            Some(Command::Wallet(options)) => {
                let wallet = WalletService::open().await?;
                match options.action() {
                    WalletAction::ShowPortfolio => wallet.show_portfolio().await?,
                    WalletAction::ShowPaths => wallet.show_paths(),
                    WalletAction::Reset { skip_confirmation } => {
                        wallet.reset(skip_confirmation).await?;
                    }
                }
            }
            None => print_about(),
        }
        Ok(())
    }
}

/// The bare-`yd` landing screen.
///
/// Only the identity block is shown; module hints stay inside the root help
/// text. The module list prints once a second module is registered, so the
/// first screen never advertises a single command that already has its own
/// dedicated alias help.
fn print_about() {
    println!("{}", Ui::text(Tone::Brand, "^.^"));
    Ui::divider();
    Ui::kv("Version", env!("CARGO_PKG_VERSION"));
    Ui::kv("License", "MIT");
    Ui::kv("Author", "t.me/n0s3nse");
    Ui::kv("Source", "github.com/9sx77ssl/yd");
    Ui::divider();
    if MODULES.len() > 1 {
        for module in MODULES {
            println!(
                "{}  {}",
                Ui::text(Tone::Heading, module.command),
                Ui::text(Tone::Muted, format!("yd {}", module.short_alias()))
            );
        }
        Ui::divider();
    }
}
