use std::{fs, path::PathBuf};

use anyhow::{anyhow, Result};
use clap::Args;
use config::{BtcConfig, Config};
#[cfg(feature = "celestia")]
use da::CelestiaConfig;
use da::DaType;
#[cfg(feature = "ethereum")]
use da::EthereumConfig;
#[cfg(feature = "file")]
use da::FileConfig;
#[cfg(feature = "greenfield")]
use da::GreenfieldConfig;
#[cfg(feature = "ipfs")]
use da::IpfsConfig;

#[derive(Debug, Args)]
pub struct GenerateConfig {
    #[clap(short, long)]
    config: String,
}

impl GenerateConfig {
    pub fn exeute(&self) -> Result<()> {
        let file = PathBuf::from(&self.config);
        if file.exists() {
            Err(anyhow!("Configuration file already exists"))
        } else {
            let cfg = Config {
                default_da: DaType::default(),
                #[cfg(feature = "file")]
                file: Some(FileConfig {
                    path: "/path/to/data".to_string(),
                }),
                #[cfg(feature = "ipfs")]
                ipfs: Some(IpfsConfig {
                    url: "http://127.0.0.1:5001".to_string(),
                }),
                #[cfg(feature = "celestia")]
                celestia: Some(CelestiaConfig {
                    url: "http://127.0.0.1:8080".to_string(),
                    token: "vefbrebqrber".to_string(),
                    id: "12345".to_string(),
                }),
                #[cfg(feature = "greenfield")]
                greenfield: Some(GreenfieldConfig {
                    rpc_addr: "https://gnfd-testnet-fullnode-tendermint-us.bnbchain.org:443"
                        .to_string(),
                    chain_id: "greenfield_5600-1".to_string(),
                    bucket: "bucketname".to_string(),
                    password_file: "/tmp/password".to_string(),
                }),
                #[cfg(feature = "ethereum")]
                ethereum: Some(EthereumConfig {
                    url: "http://127.0.0.1:8545".to_string(),
                    to: "0x0000000000000000000000000000000000000001".to_string(),
                    sk: "0x24e196d2883a86572d43f7896d6ffd0c11a456afba1c1c3180674b6f0624cace"
                        .to_string(),
                }),
                btc: BtcConfig {
                    electrs_url: "tcp://127.0.0.1:60401".to_string(),
                    btc_url: "http://127.0.0.1:18443".to_string(),
                    username: "admin1".to_string(),
                    password: "123".to_string(),
                    network: "regtest".to_string(),
                    da_fee: 100,
                    fee_address: "bcrt1qhwkqamxr93phyhlc82elqm2n8hufr8xls0djwn".to_string(),
                },
            };
            Ok(fs::write(file, toml::to_string_pretty(&cfg)?)?)
        }
    }
}
