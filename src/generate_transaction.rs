#![deny(warnings)]

mod config;
mod tx;
mod vout_code;

use clap::Parser;
use da::create_da_mgr;

use config::Config;
use ethers::types::{H160, U256};
use rt_evm::model::codec::{hex_decode, hex_encode};
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
    pub address: String,
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
        cfg.file_path.as_deref(),
        cfg.ipfs,
        cfg.ipfs_url.as_deref(),
        cfg.celestia,
        cfg.celestia_url.as_deref(),
        cfg.celestia_token.as_deref(),
        cfg.celestia_namespace_id.as_deref(),
        cfg.greenfield,
        cfg.greenfield_rpc_addr.as_deref(),
        cfg.greenfield_chain_id.as_deref(),
        cfg.greenfield_bucket.as_deref(),
        cfg.greenfield_password_file.as_deref(),
        &cfg.default,
    )
    .await
    .map_err(|e| eg!(e))?;
    let btc_builder =
        BtcTransactionBuilder::new(&cfg.electrs_url, &cfg.btc_url, &cfg.username, &cfg.password)
            .await?;
    let eth_builder = EthTransactionBuilder::new(&cfg.eth_url, &cmd.private_key).await?;
    let data = match cmd.data {
        Some(v) => hex_decode(v.strip_prefix("0x").unwrap_or(&v)).c(d!())?,
        None => vec![],
    };
    let sig = cmd.sig.clone().unwrap_or(String::new());
    let (eth_tx, fee) = eth_builder
        .build_transaction(H160::default(), cmd.value, cmd.to, &data, &sig, cmd.args)
        .await?;
    log::info!("etc transaction:{}", hex_encode(&eth_tx));
    let chain_id = eth_builder.chain_id().await?;
    let hash = da_mgr.set_tx(&eth_tx).await.map_err(|e| eg!(e))?;
    let vc = VoutCode::new(chain_id, 0, da_mgr.default_type(), 0, &hash[1..])?;

    let txid = btc_builder
        .build_transaction(
            &cmd.private_key,
            &cfg.network,
            &cmd.address,
            fee,
            &vc.encode(),
        )
        .await?;
    println!("bitcoin transaction: {}", txid);
    Ok(())
}
