#![deny(warnings)]

mod config;
mod tx;
mod vout_code;

use clap::Parser;
use da::create_da_mgr;

use config::Config;
use ethers::types::{H160, U256};
use rt_evm_model::codec::{hex_decode, hex_encode};
use ruc::*;
use tx::{btc::BtcTransactionBuilder, eth::EthTransactionBuilder};

use crate::vout_code::VoutCode;

#[derive(Debug, Parser)]
pub struct CommandLine {
    #[clap(long)]
    pub config: String,
    #[clap(long)]
    pub private_key: String,
    #[clap(long)]
    pub txid: String,
    #[clap(long)]
    pub vout: u32,
    #[clap(long)]
    pub to: Option<H160>,
    #[clap(long)]
    pub value: U256,
    #[clap(long)]
    pub data: Option<String>,
    pub sig: Option<String>,
    pub args: Vec<String>,
}

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::init();
    let cmd = CommandLine::parse();
    let cfg = Config::new(&cmd.config)?;
    let da_mgr = create_da_mgr(
        cfg.file,
        cfg.file_path.as_ref().map(|x| x.as_str()),
        cfg.ipfs,
        cfg.ipfs_url.as_ref().map(|x| x.as_str()),
        cfg.celestia,
        cfg.celestia_url.as_ref().map(|x| x.as_str()),
        cfg.celestia_token.as_ref().map(|x| x.as_str()),
        cfg.namespace_id.as_ref().map(|x| x.as_str()),
        &cfg.default,
    )
    .await
    .map_err(|e| eg!(e))?;

    let builder = EthTransactionBuilder::new(&cfg.eth_url, &cmd.private_key).await?;
    let data = cmd
        .data
        .c(d!())
        .and_then(|d| hex_decode(&d.strip_prefix("0x").unwrap_or(&d)).c(d!()))?;
    let sig = cmd.sig.clone().unwrap_or(String::new());
    let (eth_tx, mut fee) = builder
        .build_transaction(cmd.value, cmd.to, &data, &sig, cmd.args)
        .await?;
    log::info!("etc transaction:{}", hex_encode(&eth_tx));
    if fee < cfg.fee {
        fee = 2000;
    }
    let chain_id = builder.chain_id();
    let hash = da_mgr.set_tx(&eth_tx).await.map_err(|e| eg!(e))?;
    let vc = VoutCode::new(chain_id, 0, da_mgr.default_type(), 0, &hash[1..])?;

    let builder = BtcTransactionBuilder::new(
        &cfg.btc_url,
        &cfg.username,
        &cfg.password,
        &cmd.private_key,
        &cfg.network,
    )
    .await?;

    let txid = builder
        .build_transaction(fee, &vc.encode(), cmd.txid, cmd.vout)
        .await?;
    println!("bitcoin transaction: {}", txid);
    Ok(())
}
