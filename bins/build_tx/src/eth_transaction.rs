use anyhow::{anyhow, Result};
use bitcoin::{consensus::serialize, Address, PrivateKey};
use bitcoincore_rpc::{Auth, Client};
use clap::Args;
use config::Config;
use da::DaType;
#[cfg(feature = "file")]
use da::FileService;
#[cfg(feature = "greenfield")]
use da::GreenfieldService;
use ethers::types::{Bytes, H160, U256};
use std::sync::Arc;
use tx_builder::{btc::BtcTransactionBuilder, eth::EthTransactionBuilder, SAT2WEI};
use utils::ScriptCode;

#[derive(Debug, Args)]
/// build eth transaction
pub struct EthTransaction {
    #[clap(long)]
    to: Option<H160>,
    #[clap(long)]
    value: U256,
    #[clap(long)]
    data: Option<String>,
}

impl EthTransaction {
    pub async fn execute(
        &self,
        cfg: Config,
        novo_api_url: &str,
        eth_url: String,
        private_key: PrivateKey,
        address: Address,
    ) -> Result<Option<(Bytes, Bytes)>> {
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

        let eth_builder = EthTransactionBuilder::new(&eth_url)?;
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

        let mut sc = ScriptCode::default();
        sc.chain_id = eth_builder.chain_id().await?;
        sc.tx_type = 1;
        sc.da_type = cfg.default_da.type_byte();
        sc.hash = match cfg.default_da {
            #[cfg(feature = "file")]
            DaType::File => FileService::hash(&eth_tx_bytes),
            #[cfg(feature = "ipfs")]
            DaType::Ipfs => todo!(),
            #[cfg(feature = "celestia")]
            DaType::Celestia => todo!(),
            #[cfg(feature = "greenfield")]
            DaType::Greenfield => GreenfieldService::hash(&eth_tx_bytes),
        };

        let btc_tx = btc_builder
            .build_transaction(
                novo_api_url,
                private_key,
                script,
                unspents,
                gas,
                &sc.encode(),
            )
            .await?;
        log::info!("btc transaction:{:#?}", btc_tx);

        Ok(Some((eth_tx_bytes, Bytes::from_iter(serialize(&btc_tx)))))
    }
}
