use bip39::Mnemonic;
use color_eyre::eyre::{Result, WrapErr};
use secrecy::{ExposeSecret, SecretString};

use super::address::WalletKeys;
use super::model::PortfolioEntry;
use super::provider::wallet_providers;
use super::store::WalletStore;
use crate::error::YdError;
use crate::net::PriceService;
use crate::store::Database;
use crate::ui::{Tone, Ui};

pub struct WalletService {
    store: WalletStore,
    prices: PriceService,
}

impl WalletService {
    pub async fn open() -> Result<Self> {
        let database = Database::open()?;
        database.migrate(WalletStore::MIGRATIONS).await?;
        let prices = PriceService::new(database.clone());
        Ok(Self {
            store: WalletStore::new(database),
            prices,
        })
    }

    pub fn show_paths(&self) {
        Ui::title("Wallet paths");
        Ui::divider();
        println!(
            "{}  {}",
            Ui::text(Tone::Heading, "Storage"),
            Ui::text(
                Tone::Muted,
                self.store.database().database_path().display().to_string()
            )
        );
        Ui::divider();
        let paths = [
            ("Ethereum", "m/44'/60'/0'/0/0"),
            ("BNB Chain", "m/44'/60'/0'/0/0"),
            ("Polygon", "m/44'/60'/0'/0/0"),
            ("Bitcoin", "m/84'/0'/0'/0/0"),
            ("Litecoin", "m/44'/2'/0'/0/0"),
            ("TON", "m/44'/607'/0'/0'"),
        ];
        for (name, path) in paths {
            println!(
                "{}  {}",
                Ui::text(Tone::Heading, name),
                Ui::text(Tone::Muted, path)
            );
        }
        Ui::divider();
    }

    pub async fn show_portfolio(&self) -> Result<()> {
        let phrase = match self.store.load_phrase().await? {
            Some(phrase) => {
                Ui::title("Wallet");
                Ui::divider();
                phrase
            }
            None => self.configure_wallet().await?,
        };
        let phrase = phrase.expose_secret().to_owned();
        let _keys = WalletKeys::from_mnemonic(&phrase)?;
        let seed = mnemonic_to_seed(&phrase);
        let providers = wallet_providers(self.prices.clone(), seed);

        let mut total_usd = 0.0;
        let mut has_total = false;

        for provider in &providers {
            tracing::debug!("scanning {:?}", provider.kind());
            match provider.fetch_all().await {
                Ok(entries) => {
                    for entry in entries {
                        if !entry.has_balance() {
                            continue;
                        }
                        if let Some(usd_value) = entry.usd_value {
                            total_usd += usd_value;
                            has_total = true;
                        }
                        print_entry(&entry);
                    }
                }
                Err(error) => {
                    tracing::debug!("{error}");
                }
            }
        }
        if has_total {
            println!(
                "{}  {} ${total_usd:.2}",
                Ui::text(Tone::Heading, "Total"),
                Ui::text(Tone::Muted, "≈")
            );
            Ui::divider();
        }
        Ok(())
    }

    pub async fn reset(&self, skip_confirmation: bool) -> Result<()> {
        if !self.store.has_wallet().await? {
            Ui::warning("No wallet is stored on this machine.");
            return Ok(());
        }
        if !skip_confirmation && !Ui::confirm("Remove the encrypted wallet from this machine?")? {
            println!("{}", Ui::text(Tone::Muted, "Reset cancelled."));
            return Ok(());
        }
        self.store.remove_wallet().await?;
        Ui::success("Wallet removed from this machine.");
        Ok(())
    }

    async fn configure_wallet(&self) -> Result<SecretString> {
        Ui::title("Wallet");
        println!(
            "{}",
            Ui::text(Tone::Muted, "No wallet stored on this machine.")
        );
        let phrase = rpassword::prompt_password("Seed phrase (hidden): ")?
            .trim()
            .to_owned();
        let mnemonic = phrase
            .parse::<Mnemonic>()
            .map_err(|error| YdError::InvalidMnemonic(error.to_string()))?;
        self.store
            .save_phrase(SecretString::from(mnemonic.to_string()))
            .await
            .wrap_err("could not save encrypted wallet")?;
        Ui::success("Wallet saved locally and encrypted.");
        Ui::divider();
        Ok(SecretString::from(phrase))
    }
}

fn mnemonic_to_seed(phrase: &str) -> [u8; 64] {
    let mnemonic = phrase.parse::<Mnemonic>().expect("already validated");
    let seed = mnemonic.to_seed("");
    let mut bytes = [0u8; 64];
    bytes.copy_from_slice(&seed);
    bytes
}

fn print_entry(entry: &PortfolioEntry) {
    println!(
        "{}  {}",
        Ui::text(Tone::Heading, entry.name),
        Ui::text(Tone::Muted, entry.symbol)
    );
    println!("{}", Ui::text(Tone::Muted, &entry.address));
    let value = match entry.usd_value {
        Some(usd) => format!(
            "{}  {}  {} ${usd:.2}",
            entry.balance,
            entry.symbol,
            Ui::text(Tone::Muted, "≈")
        ),
        None => format!("{}  {}", entry.balance, entry.symbol),
    };
    println!("{}", Ui::text(Tone::Value, value));
    Ui::divider();
}
