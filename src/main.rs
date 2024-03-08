#![deny(warnings, unused_crate_dependencies)]

mod command_line;
mod generate_config;
mod node;

use anyhow::Result;
use clap::Parser;
use command_line::CommandLine;

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::init();
    let cmd = CommandLine::parse();
    cmd.exeute().await
}
