use clap::{ArgGroup, Args, CommandFactory, FromArgMatches, Parser, Subcommand};
use std::ffi::OsString;

use crate::commands;

#[derive(Debug, Parser)]
#[command(
    name = "yd",
    version,
    about = "A personal terminal multitool",
    long_about = "yd is a personal terminal multitool. Wallet is the first module.",
    override_usage = "yd [OPTION]",
    color = clap::ColorChoice::Auto
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Command>,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Cryptocurrency wallet portfolio and controls
    #[command(hide = true)]
    Wallet(WalletArgs),
}

#[derive(Debug, Args)]
#[command(
    about = "Show balances and manage the local wallet",
    override_usage = "yd -w [OPTION]",
    group(
        ArgGroup::new("wallet_action")
            .args(["paths", "reset"])
            .multiple(false)
    )
)]
pub struct WalletArgs {
    /// Show wallet derivation paths without fetching balances
    #[arg(short, long, conflicts_with = "reset")]
    pub paths: bool,

    /// Remove the locally stored wallet after confirmation
    #[arg(short, long)]
    pub reset: bool,

    /// Skip the reset confirmation prompt
    #[arg(short, long, requires = "reset")]
    pub yes: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WalletAction {
    ShowPortfolio,
    ShowPaths,
    Reset { skip_confirmation: bool },
}

impl WalletArgs {
    pub const fn action(&self) -> WalletAction {
        if self.paths {
            WalletAction::ShowPaths
        } else if self.reset {
            WalletAction::Reset {
                skip_confirmation: self.yes,
            }
        } else {
            WalletAction::ShowPortfolio
        }
    }
}

pub fn parse() -> Cli {
    parse_from(std::env::args_os().collect())
}

pub fn parse_from(arguments: Vec<OsString>) -> Cli {
    let command = Cli::command().after_help(commands::root_help());
    let matches = command.get_matches_from(commands::normalize_arguments(arguments));
    Cli::from_arg_matches(&matches).expect("clap returned matches compatible with Cli")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn try_parse(arguments: &[&str]) -> clap::error::Result<Cli> {
        let command = Cli::command().after_help(commands::root_help());
        let arguments = arguments.iter().map(OsString::from).collect();
        let matches = command.try_get_matches_from(commands::normalize_arguments(arguments))?;
        Cli::from_arg_matches(&matches)
    }

    #[test]
    fn wallet_paths_maps_to_typed_action() {
        let cli = try_parse(&["yd", "-w", "-p"]).unwrap();
        let Some(Command::Wallet(args)) = cli.command else {
            panic!("expected wallet command");
        };
        assert_eq!(args.action(), WalletAction::ShowPaths);
    }

    #[test]
    fn wallet_yes_requires_reset() {
        let error = try_parse(&["yd", "-w", "-y"]).unwrap_err();
        assert_eq!(
            error.kind(),
            clap::error::ErrorKind::MissingRequiredArgument
        );
    }

    #[test]
    fn wallet_paths_conflicts_with_reset_in_any_order() {
        for arguments in [
            &["yd", "-w", "-p", "-r"][..],
            &["yd", "-w", "-r", "-p"],
            &["yd", "-w", "-p", "-r", "-y"],
            &["yd", "-w", "-r", "-y", "-p"],
        ] {
            let error = try_parse(arguments).unwrap_err();
            assert_eq!(error.kind(), clap::error::ErrorKind::ArgumentConflict);
        }
    }

    #[test]
    fn wallet_reset_accepts_yes_in_any_order() {
        for arguments in [&["yd", "-w", "-r", "-y"][..], &["yd", "-w", "-y", "-r"]] {
            let cli = try_parse(arguments).unwrap();
            let Some(Command::Wallet(args)) = cli.command else {
                panic!("expected wallet command");
            };
            assert_eq!(
                args.action(),
                WalletAction::Reset {
                    skip_confirmation: true
                }
            );
        }
    }
}
