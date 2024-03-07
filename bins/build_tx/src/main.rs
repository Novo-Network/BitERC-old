#![deny(warnings, unused_crate_dependencies)]

mod command_line;
mod config_transaction;
mod eth_transaction;

use anyhow::Result;
use clap::Parser;
use command_line::CommandLine;

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::init();
    let cmd = CommandLine::parse();
    cmd.execute().await
}
