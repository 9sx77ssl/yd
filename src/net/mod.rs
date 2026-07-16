//! Shared networking layer.
//!
//! [`ApiService`] wraps `reqwest` with typed error attribution so any domain
//! (wallet, weather, IP lookup) reports API failures consistently. The
//! [`fallback`] module generalises the resilient primary/fallback query
//! pattern used by the price feed; [`PriceService`] caches USD quotes in the
//! shared TTL cache so repeated runs stay fast and avoid noisy provider limits.

pub mod client;
pub mod fallback;
pub mod price;

pub use client::{shared_client, ApiService};
pub use price::{Asset, PriceService};
