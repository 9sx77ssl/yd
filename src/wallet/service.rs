use std::sync::Arc;

use bip39::Mnemonic;
use color_eyre::eyre::{Result, WrapErr};
use secrecy::{ExposeSecret, SecretString};

use super::address::WalletKeys;
use super::model::PortfolioEntry;
use super::provider::{wallet_providers, NetworkProvider};
use super::store::WalletStore;
use crate::error::YdError;
use crate::net::PriceService;
use crate::store::Database;
use crate::ui::{Tone, Ui};

pub struct WalletService {
    store: WalletStore,
    providers: Vec<Arc<dyn NetworkProvider>>,
}

impl WalletService {
    pub async fn open() -> Result<Self> {
        let database = Database::open()?;
        database.migrate(WalletStore::MIGRATIONS).await?;
        let prices = PriceService::new(database.clone());
        Ok(Self {
            store: WalletStore::new(database),
            providers: wallet_providers(prices),
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
        for provider in &self.providers {
            println!(
                "{}  {}",
                Ui::text(Tone::Heading, provider.name()),
                Ui::text(Tone::Muted, provider.kind().derivation_path())
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
        let keys = WalletKeys::from_mnemonic(&phrase)?;
        let jobs = self
            .providers
            .iter()
            .map(|provider| provider.fetch(keys.address_for(provider.kind())));
        let results = futures::future::join_all(jobs).await;

        let mut total_usd = 0.0;
        let mut has_total = false;

        for (provider, result) in self.providers.iter().zip(results) {
            match result {
                Ok(entry) => {
                    if let Some(usd_value) = entry.usd_value {
                        total_usd += usd_value;
                        has_total = true;
                    }
                    print_entry(&entry);
                }
                Err(error) => Ui::warning(&format!("{} unavailable: {error}", provider.name())),
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
