mod crypto;
mod provider;
mod store;

use std::sync::Arc;

use bip39::Mnemonic;
use color_eyre::eyre::{Result, WrapErr};
use owo_colors::OwoColorize;

use crate::error::YdError;
use crypto::WalletKeys;
use provider::{
    BitcoinProvider, EthereumProvider, LitecoinProvider, NetworkProvider, PortfolioEntry,
};
use store::WalletStore;

pub struct WalletService {
    store: WalletStore,
    providers: Vec<Arc<dyn NetworkProvider>>,
}

impl WalletService {
    pub fn open() -> Result<Self> {
        Ok(Self {
            store: WalletStore::open()?,
            providers: vec![
                Arc::new(EthereumProvider::new()),
                Arc::new(BitcoinProvider::new()),
                Arc::new(LitecoinProvider::new()),
            ],
        })
    }

    pub async fn show_portfolio(&self) -> Result<()> {
        let phrase = match self.store.load_phrase().await? {
            Some(phrase) => phrase,
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
                Err(error) => eprintln!(
                    "{} {}: {}",
                    "!".yellow().bold(),
                    provider.name().bold(),
                    error.to_string().red()
                ),
            }
        }
        Ok(())
    }

    async fn configure_wallet(&self) -> Result<String> {
        println!("{} No wallet configured yet.", "yd".bold().cyan());
        let phrase = rpassword::prompt_password("Enter your BIP-39 seed phrase: ")?
            .trim()
            .to_owned();
        let mnemonic = phrase
            .parse::<Mnemonic>()
            .map_err(|error| YdError::InvalidMnemonic(error.to_string()))?;
        self.store
            .save_phrase(mnemonic.to_string())
            .await
            .wrap_err("could not save encrypted wallet")?;
        println!(
            "{} Wallet secured in local encrypted storage.\n",
            "✓".green().bold()
        );
        Ok(phrase)
    }
}

fn print_entry(entry: &PortfolioEntry) {
    println!("{} {}\n", "^.^".cyan().bold(), entry.name.bold());
    println!("{}\n{}\n", "Address".dimmed(), entry.address.bold());
    println!(
        "{}\n{} {}",
        "Balance".dimmed(),
        entry.balance.bold(),
        entry.symbol
    );
    match entry.usd_value {
        Some(value) => println!("{} ${value:.2}\n", "≈".dimmed()),
        None => println!(),
    }
}
