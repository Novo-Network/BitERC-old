use anyhow::{anyhow, Result};
use bitcoin::consensus::serialize;
use bitcoincore_rpc::{Auth, Client};
use clap::Args;
use config::Config;
use da::DAServiceManager;
#[cfg(feature = "file")]
use da::FileService;
#[cfg(feature = "greenfield")]
use da::GreenfieldService;
use ethers::types::{Bytes, H160, U256};
use json_rpc_server::call;
use serde_json::json;
use std::sync::Arc;
use tx_builder::{btc::BtcTransactionBuilder, eth::EthTransactionBuilder, SAT2WEI};
use utils::ScriptCode;

#[derive(Debug, Args)]
/// build eth transaction
pub struct EthTransaction {
    #[clap(short, long)]
    config: String,

    #[clap(short, long)]
    private_key: String,

    #[clap(short, long)]
    eth_url: String,

    #[clap(short, long)]
    novo_api_url: String,

    #[arg(short, long)]
    send_tx: bool,

    #[clap(short, long)]
    to: Option<H160>,

    #[clap(short, long)]
    value: U256,

    #[clap(short, long)]
    data: Option<String>,
}

impl EthTransaction {
    pub async fn execute(&self) -> Result<()> {
        let cfg = Config::new(&self.config)?;

        let (private_key, address) =
            BtcTransactionBuilder::parse_sk(&self.private_key, &cfg.btc.network)?;

        let client = Arc::new(Client::new(
            &cfg.btc.btc_url,
            Auth::UserPass(cfg.btc.username.clone(), cfg.btc.password.clone()),
        )?);
        let btc_builder = BtcTransactionBuilder::new(&cfg.btc.electrs_url, client)?;

        let data = match &self.data {
            Some(v) => hex::decode(v.strip_prefix("0x").unwrap_or(&v))?,
            None => vec![],
        };

        let script = address.script_pubkey();
        let unspents = btc_builder.list_unspent(&script)?;
        let from = {
            let first_input = unspents.first().ok_or(anyhow!("input not found"))?;
            btc_builder.get_eth_from_address(&first_input.tx_hash, first_input.tx_pos as u32)?
        };

        let eth_builder = EthTransactionBuilder::new(&self.eth_url)?;
        let eth_tx = eth_builder
            .build_transaction(from, self.value, self.to, &data)
            .await?;
        log::info!("etc transaction:{:#?}", eth_tx);

        let gas = eth_tx
            .gas()
            .and_then(|v| v.checked_div(U256::from(SAT2WEI)))
            .map(|v| v.as_u64())
            .ok_or(anyhow!("build transaction error"))?;

        let eth_tx_bytes = eth_tx.rlp();

        let da_mgr = Arc::new(
            DAServiceManager::new(
                cfg.default_da,
                #[cfg(feature = "file")]
                cfg.file,
                #[cfg(feature = "ipfs")]
                cfg.ipfs,
                #[cfg(feature = "celestia")]
                cfg.celestia,
                #[cfg(feature = "greenfield")]
                cfg.greenfield,
                #[cfg(feature = "ethereum")]
                cfg.ethereum,
            )
            .await?,
        );

        let mut sc = ScriptCode::default();
        sc.chain_id = eth_builder.chain_id().await?;
        sc.tx_type = 1;
        sc.da_type = da_mgr.default_type();
        sc.hash = da_mgr.calc_hash(&eth_tx_bytes).await?;

        let btc_tx = btc_builder
            .build_transaction(
                &self.novo_api_url,
                private_key,
                script,
                unspents,
                gas,
                &sc.encode(),
            )
            .await?;
        log::info!("btc transaction:{:#?}", btc_tx);

        let (tx_data, btc_tx) = (eth_tx_bytes, Bytes::from_iter(serialize(&btc_tx)));
        if self.send_tx {
            let txid: Option<String> = call(
                &self.novo_api_url,
                "novo_sendRawTransaction",
                &vec![tx_data, btc_tx],
                None,
            )
            .await
            .map_err(|e| anyhow!("{:?}", e))?;
            println!("send transaction sucess: {:?}", txid);
        } else {
            println!(
                "{}",
                serde_json::to_string_pretty(&json!({
                    "tx_data": tx_data,
                    "btc_tx": btc_tx,
                }))?
            );
        }
        Ok(())
    }
}
