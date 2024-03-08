use anyhow::Result;
use clap::{Parser, Subcommand};

use crate::{generate_config::GenerateConfig, node::Node};

#[derive(Subcommand)]
pub enum Command {
    GenCfg(GenerateConfig),
    Node(Node),
}

#[derive(Parser)]
pub struct CommandLine {
    #[command(subcommand)]
    command: Command,
}

impl CommandLine {
    pub async fn exeute(&self) -> Result<()> {
        match &self.command {
            Command::GenCfg(c) => c.exeute(),
            Command::Node(c) => c.exeute().await,
        }
    }
}
