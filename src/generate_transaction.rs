#![deny(warnings)]

mod config;
mod tx;
mod vout_code;

use std::{str::FromStr, sync::Arc};

use bitcoin::consensus::serialize;
use bitcoincore_rpc::{Auth, Client as BitcoincoreClient};
use clap::Parser;

use config::Config;
use da::{DaType, FileService, GreenfieldService};
use ethers::types::{H160, U256};
use rt_evm::model::codec::{hex_decode, hex_encode};
use ruc::*;
use services::jsonrpc::call;
use tx::{btc::BtcTransactionBuilder, eth::EthTransactionBuilder};

use crate::{tx::SAT2WEI, vout_code::VoutCode};

#[derive(Debug, Parser)]
pub struct CommandLine {
    #[clap(long)]
    pub config: String,
    #[clap(long)]
    pub eth_url: String,
    #[clap(long)]
    pub send_tx_url: String,
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

    let da_type = DaType::from_str(&cfg.default).map_err(|e| eg!(e))?;
    let bitcoincore_client = Arc::new(
        BitcoincoreClient::new(
            &cfg.btc_url,
            Auth::UserPass(cfg.username.clone(), cfg.password.clone()),
        )
        .c(d!())?,
    );
    let btc_builder = BtcTransactionBuilder::new(&cfg.electrs_url, bitcoincore_client).await?;
    let eth_builder = EthTransactionBuilder::new(&cmd.eth_url, &cmd.private_key).await?;
    let data = match cmd.data {
        Some(v) => hex_decode(v.strip_prefix("0x").unwrap_or(&v)).c(d!())?,
        None => vec![],
    };
    let sig = cmd.sig.clone().unwrap_or(String::new());

    let unspents = btc_builder.list_unspent(&cmd.address)?;
    let tmp = unspents.first().c(d!())?;
    let from = btc_builder.get_eth_from_address(&tmp.tx_hash, tmp.tx_pos as u32)?;
    let eth_tx = eth_builder
        .build_transaction(from, cmd.value, cmd.to, &data, &sig, cmd.args)
        .await?;
    log::info!("etc transaction:{:#?}", eth_tx);
    let gas = eth_tx
        .gas()
        .and_then(|v| v.checked_div(U256::from(SAT2WEI)))
        .map(|v| v.as_u64())
        .c(d!())?;

    let eth_tx_bytes = eth_tx.rlp();
    let chain_id = eth_builder.chain_id().await?;
    let hash = match da_type {
        DaType::File => FileService::hash(&eth_tx_bytes),
        DaType::Greenfield => GreenfieldService::hash(&eth_tx_bytes),
        _ => return Err(eg!("default can only be file greenfield")),
    };

    let vc = VoutCode::new(chain_id, 0, da_type.type_byte(), 0, &hash)?;

    let btc_tx = btc_builder
        .build_transaction(
            &cmd.private_key,
            &cfg.network,
            &cmd.address,
            unspents,
            gas,
            &vc.encode(),
        )
        .await?;
    log::info!("btc transaction:{:#?}", btc_tx);
    let btc_tx_bytes = serialize(&btc_tx);
    let txid: Option<String> = call(
        &cmd.send_tx_url,
        "api_sendRawTransaction",
        &vec![hex_encode(eth_tx_bytes), hex_encode(btc_tx_bytes)],
        None,
    )
    .await
    .map_err(|e| eg!("{:?}", e))?;

    println!("eth da hash:{}", hex_encode(hash));
    println!("send btc transaction:{:?}", txid);

    Ok(())
}
