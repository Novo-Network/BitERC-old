use anyhow::{anyhow, Result};
use clap::{Parser, Subcommand};
use config::Config;
use json_rpc_server::call;
use serde_json::json;
use tx_builder::btc::BtcTransactionBuilder;

use crate::{config_transaction::ConfigTransaction, eth_transaction::EthTransaction};

#[derive(Subcommand)]
pub enum Commands {
    ConfigTransaction(ConfigTransaction),
    ETHTransaction(EthTransaction),
}

#[derive(Parser)]
pub struct CommandLine {
    #[clap(long)]
    config: String,

    #[clap(long)]
    private_key: String,

    #[clap(long)]
    eth_url: String,

    /// After specifying the parameters, the transaction will be automatically sent
    #[clap(long)]
    novo_api_url: String,

    #[arg(short, long)]
    send_tx: bool,

    #[command(subcommand)]
    command: Commands,
}

impl CommandLine {
    pub async fn execute(self) -> Result<()> {
        let cfg = Config::new(&self.config)?;

        let (private_key, address) =
            BtcTransactionBuilder::parse_sk(&self.private_key, &cfg.btc.network)?;

        let (eth_tx, btc_tx) = if let Some((eth_tx, btc_tx)) = match self.command {
            Commands::ConfigTransaction(c) => {
                c.execute(cfg, &self.novo_api_url, self.eth_url, private_key, address)
                    .await?
            }
            Commands::ETHTransaction(c) => {
                c.execute(cfg, &self.novo_api_url, self.eth_url, private_key, address)
                    .await?
            }
        } {
            (eth_tx, btc_tx)
        } else {
            return Ok(());
        };

        if self.send_tx {
            let txid: Option<String> = call(
                &self.novo_api_url,
                "novo_sendRawTransaction",
                &vec![eth_tx, btc_tx],
                None,
            )
            .await
            .map_err(|e| anyhow!("{:?}", e))?;
            println!("send transaction sucess: {:?}", txid);
        } else {
            println!(
                "{}",
                serde_json::to_string_pretty(&json!({
                   "eth_tx": eth_tx,
                    "btc_tx": btc_tx,
                }))?
            );
        }
        Ok(())
    }
}
