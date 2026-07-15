use std::sync::Arc;

use bip39::Mnemonic;
use color_eyre::eyre::{Result, WrapErr};

use super::{
    crypto::WalletKeys,
    model::{EvmNetworkConfig, PortfolioEntry},
    provider::{BitcoinProvider, EvmProvider, LitecoinProvider, NetworkProvider, PriceService},
    store::WalletStore,
};
use crate::{
    error::YdError,
    ui::{Tone, Ui},
};

pub struct WalletService {
    store: WalletStore,
    providers: Vec<Arc<dyn NetworkProvider>>,
}

impl WalletService {
    pub fn open() -> Result<Self> {
        let store = WalletStore::open()?;
        let prices = PriceService::new(store.clone());
        Ok(Self {
            store,
            providers: vec![
                Arc::new(EvmProvider::new(
                    EvmNetworkConfig::ethereum(),
                    prices.clone(),
                )),
                Arc::new(EvmProvider::new(
                    EvmNetworkConfig::bnb_chain(),
                    prices.clone(),
                )),
                Arc::new(BitcoinProvider::new(prices.clone())),
                Arc::new(LitecoinProvider::new(prices)),
            ],
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
                self.store.database_path().display().to_string()
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
        if !skip_confirmation && !confirm_reset()? {
            println!("{}", Ui::text(Tone::Muted, "Reset cancelled."));
            return Ok(());
        }
        self.store.remove_wallet().await?;
        Ui::success("Wallet removed from this machine.");
        Ok(())
    }

    async fn configure_wallet(&self) -> Result<String> {
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
            .save_phrase(mnemonic.to_string())
            .await
            .wrap_err("could not save encrypted wallet")?;
        Ui::success("Wallet saved locally and encrypted.");
        Ui::divider();
        Ok(phrase)
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

fn confirm_reset() -> Result<bool> {
    use std::io::{self, Write};

    print!("Remove the encrypted wallet from this machine? [y/N] ");
    io::stdout().flush()?;
    let mut answer = String::new();
    io::stdin().read_line(&mut answer)?;
    Ok(matches!(
        answer.trim().to_ascii_lowercase().as_str(),
        "y" | "yes"
    ))
}
