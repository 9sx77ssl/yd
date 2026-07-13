use crate::{
    cli::Cli,
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
        if self.cli.wallet.wallet {
            let wallet = WalletService::open()?;
            if self.cli.wallet.reset {
                wallet.reset(self.cli.wallet.yes).await?;
            } else {
                wallet.show_portfolio().await?;
            }
        } else {
            print_about();
        }
        Ok(())
    }
}

fn print_about() {
    Ui::title("yd");
    println!("{}", Ui::text(Tone::Muted, "Personal terminal multitool"));
    Ui::divider();
    println!(
        "{}  {}",
        Ui::text(Tone::Label, "Version"),
        Ui::text(Tone::Value, env!("CARGO_PKG_VERSION"))
    );
    println!(
        "{}  {}",
        Ui::text(Tone::Label, "License"),
        Ui::text(Tone::Value, "MIT")
    );
    println!(
        "{}  {}",
        Ui::text(Tone::Label, "Author"),
        Ui::text(Tone::Value, "@n0s3nse")
    );
    println!(
        "{}  {}",
        Ui::text(Tone::Label, "Source"),
        Ui::text(Tone::Value, "github.com/9sx77ssl/yd")
    );
    Ui::divider();
    println!(
        "{}  {}",
        Ui::text(Tone::Heading, "Wallet"),
        Ui::text(Tone::Muted, "yd -w")
    );
}
