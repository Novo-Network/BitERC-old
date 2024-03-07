use std::{collections::BTreeMap, fs, path::PathBuf, sync::Arc};

use anyhow::{anyhow, Result};
use bitcoin::{consensus::serialize, Address, PrivateKey};
use bitcoincore_rpc::{Auth, Client};
use clap::Args;
use config::{Account, ChainConfig, Config};
use da::DaType;
#[cfg(feature = "file")]
use da::FileService;
#[cfg(feature = "greenfield")]
use da::GreenfieldService;
use ethers::{
    providers::{Http, Middleware, Provider},
    types::{Bytes, H160, H256, U256},
};
use serde_json::Value;
use tx_builder::btc::BtcTransactionBuilder;
use utils::ScriptCode;

#[derive(Debug, Args)]
/// build config transaction
pub struct ConfigTransaction {
    ///Generate example configuration file
    #[arg(short, long)]
    generate: bool,
    #[arg(short, long)]
    file: String,
}
impl ConfigTransaction {
    pub async fn execute(
        &self,
        cfg: Config,
        novo_api_url: &str,
        eth_url: String,
        private_key: PrivateKey,
        address: Address,
    ) -> Result<Option<(Bytes, Bytes)>> {
        let file = PathBuf::from(&self.file);
        if self.generate {
            self.generate_cfg(file).await
        } else {
            self.build_tx(file, cfg, novo_api_url, eth_url, private_key, address)
                .await
        }
    }

    async fn generate_cfg(&self, file: PathBuf) -> Result<Option<(Bytes, Bytes)>> {
        if file.exists() {
            Err(anyhow!("Configuration file already exists"))
        } else {
            let mut storage = BTreeMap::new();
            storage.insert(U256::from(10000), U256::from(10000));
            storage.insert(U256::from(20000), U256::from(20000));

            let mut accounts = BTreeMap::new();
            accounts.insert(
                H160::default(),
                Account {
                    balance: Some(U256::from(10000)),
                    nonce: Some(U256::from(10000)),
                    code: Some(Bytes::from_static(b"example data 0")),
                    storage: Some(storage),
                },
            );
            accounts.insert(
                H160::default(),
                Account {
                    balance: Some(U256::from(20000)),
                    nonce: None,
                    code: None,
                    storage: None,
                },
            );

            let chain_cfg = ChainConfig {
                chain_id: 65535,
                bin_hash: H256::default(),
                accounts,
            };
            fs::write(file, serde_json::to_string_pretty(&chain_cfg)?)?;
            Ok(None)
        }
    }
    async fn build_tx(
        &self,
        file: PathBuf,
        cfg: Config,
        novo_api_url: &str,
        eth_url: String,
        private_key: PrivateKey,
        address: Address,
    ) -> Result<Option<(Bytes, Bytes)>> {
        let tx_bytes = {
            let content = fs::read_to_string(file)?;
            let json = serde_json::from_str::<Value>(&content)?;
            serde_json::to_vec(&json)?
        };

        let mut sc = ScriptCode::default();
        sc.chain_id = Provider::<Http>::try_from(eth_url)?
            .get_chainid()
            .await?
            .as_u32();
        sc.tx_type = 1;
        sc.da_type = cfg.default_da.type_byte();
        sc.hash = match cfg.default_da {
            #[cfg(feature = "file")]
            DaType::File => FileService::hash(&tx_bytes),
            #[cfg(feature = "ipfs")]
            DaType::Ipfs => todo!(),
            #[cfg(feature = "celestia")]
            DaType::Celestia => todo!(),
            #[cfg(feature = "greenfield")]
            DaType::Greenfield => GreenfieldService::hash(&tx_bytes),
        };

        let client = Arc::new(Client::new(
            &cfg.btc.btc_url,
            Auth::UserPass(cfg.btc.username.clone(), cfg.btc.password.clone()),
        )?);
        let btc_builder = BtcTransactionBuilder::new(&cfg.btc.electrs_url, client)?;

        let script = address.script_pubkey();
        let unspents = btc_builder.list_unspent(&script)?;
        let btc_tx = btc_builder
            .build_transaction(novo_api_url, private_key, script, unspents, 0, &sc.encode())
            .await?;

        Ok(Some((
            Bytes::from_iter(tx_bytes),
            Bytes::from_iter(serialize(&btc_tx)),
        )))
    }
}
