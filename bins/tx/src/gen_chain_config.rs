use std::{collections::BTreeMap, fs, path::PathBuf};

use anyhow::{anyhow, Result};
use clap::Args;
use config::{Account, ChainConfig};
use ethers::types::{Bytes, H160, H256, U256};

#[derive(Debug, Args)]
/// build config transaction
pub struct GenChainConfig {
    #[arg(short, long)]
    chain_config: String,
}
impl GenChainConfig {
    pub fn execute(&self) -> Result<()> {
        let file = PathBuf::from(&self.chain_config);

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
            Ok(fs::write(file, serde_json::to_string_pretty(&chain_cfg)?)?)
        }
    }
}
