mod app;
mod cli;
mod error;
mod wallet;

use clap::Parser;
use color_eyre::eyre::Result;
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> Result<()> {
    color_eyre::install()?;
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| "yd=info".into()))
        .with_target(false)
        .without_time()
        .init();

    app::Application::new(cli::Cli::parse()).run().await
}
