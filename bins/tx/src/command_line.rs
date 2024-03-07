use anyhow::Result;
use clap::{Parser, Subcommand};

use crate::{
    config_transaction::ConfigTransaction, eth_transaction::EthTransaction,
    gen_chain_config::GenChainConfig,
};

#[derive(Subcommand)]
pub enum Commands {
    GenChainCfg(GenChainConfig),
    Cfg(ConfigTransaction),
    Eth(EthTransaction),
}

#[derive(Parser)]
pub struct CommandLine {
    #[command(subcommand)]
    command: Commands,
}

impl CommandLine {
    pub async fn execute(self) -> Result<()> {
        match self.command {
            Commands::GenChainCfg(c) => c.execute(),
            Commands::Cfg(c) => c.execute().await,
            Commands::Eth(c) => c.execute().await,
        }
    }
}
