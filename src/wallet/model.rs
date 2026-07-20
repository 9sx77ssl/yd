use crate::net::Asset;

/// Derivable address families supported by the wallet.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NetworkKind {
    Ethereum,
    BnbChain,
    Polygon,
    Bitcoin,
    Litecoin,
    Solana,
}

/// Static configuration for an EVM-compatible chain.
///
/// Every EVM network (Ethereum, BNB Chain, Polygon, ...) is described by one
/// of these; a single [`super::evm::EvmProvider`] serves them all, so adding a
/// chain is a new `const fn` here, not a new provider implementation.
#[derive(Clone, Copy, Debug)]
pub struct EvmNetworkConfig {
    pub kind: NetworkKind,
    pub name: &'static str,
    pub symbol: &'static str,
    pub asset: Asset,
    pub rpc_urls: &'static [&'static str],
}

impl EvmNetworkConfig {
    pub const fn ethereum() -> Self {
        Self {
            kind: NetworkKind::Ethereum,
            name: "Ethereum",
            symbol: "ETH",
            asset: Asset::Ethereum,
            rpc_urls: &["https://eth.drpc.org"],
        }
    }

    pub const fn bnb_chain() -> Self {
        Self {
            kind: NetworkKind::BnbChain,
            name: "BNB Chain",
            symbol: "BNB",
            asset: Asset::Bnb,
            rpc_urls: &[
                "https://bsc.drpc.org",
                "https://bsc-dataseed.binance.org",
                "https://bsc-dataseed1.binance.org",
            ],
        }
    }

    pub const fn polygon() -> Self {
        Self {
            kind: NetworkKind::Polygon,
            name: "Polygon",
            symbol: "POL",
            asset: Asset::Polygon,
            rpc_urls: &["https://polygon.drpc.org"],
        }
    }
}

/// Static configuration for a UTXO chain queried through an Electrum-style
/// address API (Blockstream for Bitcoin, litecoinspace.org for Litecoin).
#[derive(Clone, Copy, Debug)]
pub struct UtxoNetworkConfig {
    pub kind: NetworkKind,
    pub name: &'static str,
    pub symbol: &'static str,
    pub asset: Asset,
    pub api_url: &'static str,
}

impl UtxoNetworkConfig {
    pub const fn bitcoin() -> Self {
        Self {
            kind: NetworkKind::Bitcoin,
            name: "Bitcoin",
            symbol: "BTC",
            asset: Asset::Bitcoin,
            api_url: "https://blockstream.info/api/address/{address}",
        }
    }

    pub const fn litecoin() -> Self {
        Self {
            kind: NetworkKind::Litecoin,
            name: "Litecoin",
            symbol: "LTC",
            asset: Asset::Litecoin,
            api_url: "https://litecoinspace.org/api/address/{address}",
        }
    }
}

/// Static configuration for a Solana network.
///
/// A single [`super::solana::SolanaProvider`] serves this chain through
/// standard `getBalance` RPC calls. Adding a Solana devnet/testnet is a new
/// `const fn` here.
#[derive(Clone, Debug)]
pub struct SolanaNetworkConfig {
    pub kind: NetworkKind,
    pub name: &'static str,
    pub symbol: &'static str,
    pub asset: Asset,
    pub rpc_urls: &'static [&'static str],
}

impl SolanaNetworkConfig {
    pub const fn mainnet() -> Self {
        Self {
            kind: NetworkKind::Solana,
            name: "Solana",
            symbol: "SOL",
            asset: Asset::Solana,
            rpc_urls: &[
                "https://api.mainnet-beta.solana.com",
                "https://solana.drpc.org",
            ],
        }
    }
}

/// One rendered row of the portfolio table.
pub struct PortfolioEntry {
    pub name: &'static str,
    pub symbol: &'static str,
    pub address: String,
    pub balance: String,
    pub usd_value: Option<f64>,
}
