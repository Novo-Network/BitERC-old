use std::{fs, sync::Arc};

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
use ethers::{
    providers::{Http, Middleware, Provider},
    types::Bytes,
};
use json_rpc_server::call;
use serde_json::{json, Value};
use tx_builder::btc::BtcTransactionBuilder;
use utils::ScriptCode;

#[derive(Debug, Args)]
/// build config transaction
pub struct ChainConfigTransaction {
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

    #[arg(short, long)]
    chain_config: String,
}
impl ChainConfigTransaction {
    pub async fn execute(&self) -> Result<()> {
        let cfg = Config::new(&self.config)?;

        let (private_key, address) =
            BtcTransactionBuilder::parse_sk(&self.private_key, &cfg.btc.network)?;

        let tx_bytes = {
            let content = fs::read_to_string(&self.chain_config)?;
            let json = serde_json::from_str::<Value>(&content)?;
            serde_json::to_vec(&json)?
        };

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

        let sc = ScriptCode {
            chain_id: Provider::<Http>::try_from(&self.eth_url)?
                .get_chainid()
                .await?
                .as_u32(),
            tx_type: 1,
            da_type: da_mgr.default_type(),
            hash: da_mgr.calc_hash(&tx_bytes).await?,
            ..Default::default()
        };

        let client = Arc::new(Client::new(
            &cfg.btc.btc_url,
            Auth::UserPass(cfg.btc.username.clone(), cfg.btc.password.clone()),
        )?);
        let btc_builder = BtcTransactionBuilder::new(&cfg.btc.electrs_url, client)?;

        let script = address.script_pubkey();
        let unspents = btc_builder.list_unspent(&script)?;

        let btc_tx = btc_builder
            .build_transaction(
                &self.novo_api_url,
                private_key,
                script,
                unspents,
                0,
                &sc.encode(),
            )
            .await?;
        let (tx_data, btc_tx) = (
            Bytes::from_iter(tx_bytes),
            Bytes::from_iter(serialize(&btc_tx)),
        );
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
