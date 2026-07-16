mod app;
mod cli;
mod commands;
mod error;
mod net;
mod secret;
mod store;
mod ui;
mod wallet;

use std::process::ExitCode;
use tracing_subscriber::EnvFilter;
use ui::Ui;

#[tokio::main]
async fn main() -> ExitCode {
    if let Err(error) = color_eyre::install() {
        Ui::error(&format!("could not initialise error reporting: {error}"));
        return ExitCode::FAILURE;
    }
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| "yd=info".into()))
        .with_target(false)
        .without_time()
        .init();

    match app::Application::new(cli::parse()).run().await {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            tracing::debug!(?error, "yd command failed");
            Ui::error(&error.to_string());
            ExitCode::FAILURE
        }
    }
}
