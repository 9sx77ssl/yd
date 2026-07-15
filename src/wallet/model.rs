#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NetworkKind {
    Ethereum,
    BnbChain,
    Bitcoin,
    Litecoin,
}

#[derive(Clone, Copy, Debug)]
pub enum Asset {
    Ethereum,
    Bnb,
    Bitcoin,
    Litecoin,
}

impl Asset {
    pub const fn coingecko_id(self) -> &'static str {
        match self {
            Self::Ethereum => "ethereum",
            Self::Bnb => "binancecoin",
            Self::Bitcoin => "bitcoin",
            Self::Litecoin => "litecoin",
        }
    }

    pub const fn symbol(self) -> &'static str {
        match self {
            Self::Ethereum => "ETH",
            Self::Bnb => "BNB",
            Self::Bitcoin => "BTC",
            Self::Litecoin => "LTC",
        }
    }
}

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
}

pub struct PortfolioEntry {
    pub name: &'static str,
    pub symbol: &'static str,
    pub address: String,
    pub balance: String,
    pub usd_value: Option<f64>,
}
