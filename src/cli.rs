use clap::Parser;

#[derive(Debug, Parser)]
#[command(name = "yd", version, about = "Your personal command-line toolkit", color = clap::ColorChoice::Auto)]
pub struct Cli {
    /// Show the cryptocurrency wallet portfolio
    #[arg(short, long)]
    pub wallet: bool,
}
