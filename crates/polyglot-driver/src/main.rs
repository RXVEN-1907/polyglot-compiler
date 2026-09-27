use clap::Parser;
use polyglot_driver::{cli::Cli, run_simple_driver};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    run_simple_driver(cli)
}
