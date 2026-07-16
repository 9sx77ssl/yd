//! Shared persistence layer.
//!
//! Every module persists into the same `yd.sqlite` database. [`Database`]
//! owns the connection profile and a versioned migration ledger, while
//! [`TtlCache`] provides expiring short-lived public data (USD quotes,
//! weather, lookup results). Domains never reach for raw SQL plumbing:
//! they compose these primitives instead of reimplementing them.

mod cache;
mod database;

pub use cache::TtlCache;
pub use database::{Database, Migration};
