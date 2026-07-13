mod crypto;
mod provider;
mod store;

use std::sync::Arc;

use bip39::Mnemonic;
use color_eyre::eyre::{Result, WrapErr};

use crate::{
    error::YdError,
    ui::{Tone, Ui},
};
use crypto::WalletKeys;
use provider::{
    BitcoinProvider, EthereumProvider, LitecoinProvider, NetworkProvider, PortfolioEntry,
    PriceService,
};
use store::WalletStore;

pub struct WalletService {
    store: WalletStore,
    providers: Vec<Arc<dyn NetworkProvider>>,
}

impl WalletService {
    pub fn open() -> Result<Self> {
        let prices = PriceService::new();
        Ok(Self {
            store: WalletStore::open()?,
            providers: vec![
                Arc::new(EthereumProvider::new(prices.clone())),
                Arc::new(BitcoinProvider::new(prices.clone())),
                Arc::new(LitecoinProvider::new(prices)),
            ],
        })
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

        for (provider, result) in self.providers.iter().zip(results) {
            match result {
                Ok(entry) => print_entry(&entry),
                Err(error) => Ui::warning(&format!("{} unavailable: {error}", provider.name())),
            }
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
