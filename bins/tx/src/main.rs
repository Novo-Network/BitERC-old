#![deny(warnings, unused_crate_dependencies)]

mod chain_config_transaction;
mod command_line;
mod eth_transaction;
mod gen_chain_config;

use anyhow::Result;
use clap::Parser;
use command_line::CommandLine;

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::init();
    let cmd = CommandLine::parse();
    cmd.execute().await
}
