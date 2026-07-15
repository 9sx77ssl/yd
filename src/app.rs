use crate::{
    cli::{Cli, Command},
    commands::{ModuleId, MODULES},
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
                let wallet = WalletService::open()?;
                if options.reset {
                    wallet.reset(options.yes).await?;
                } else if options.paths {
                    wallet.show_paths();
                } else {
                    wallet.show_portfolio().await?;
                }
            }
            None => print_about(),
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
        Ui::text(Tone::Value, "t.me/n0s3nse")
    );
    println!(
        "{}  {}",
        Ui::text(Tone::Label, "Source"),
        Ui::text(Tone::Value, "github.com/9sx77ssl/yd")
    );
    Ui::divider();
    for module in MODULES {
        let name = match module.id {
            ModuleId::Wallet => "Wallet",
        };
        println!(
            "{}  {}",
            Ui::text(Tone::Heading, name),
            Ui::text(Tone::Muted, format!("yd {}", module.short_alias))
        );
    }
}
