use crate::cli::Cli;
use crate::simple_driver::run_simple_driver;
use anyhow::Result;
use clap::Parser;

mod cli;
mod simple_driver;
mod types;

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    run_simple_driver(cli)
}