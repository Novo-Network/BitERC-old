use std::{fs::File, io::Read};

use anyhow::Result;
use da::{CelestiaConfig, DaType, FileConfig, GreenfieldConfig, IpfsConfig};
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize)]
pub struct BtcConfig {
    pub electrs_url: String,
    pub btc_url: String,
    pub username: String,
    pub password: String,
    pub network: String,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct Config {
    pub default_da: DaType,

    pub file: Option<FileConfig>,
    pub ipfs: Option<IpfsConfig>,
    pub celestia: Option<CelestiaConfig>,
    pub greenfield: Option<GreenfieldConfig>,

    pub btc: BtcConfig,
}

impl Config {
    pub fn new(path: &str) -> Result<Self> {
        let mut file = File::open(path)?;

        let mut str = String::new();
        file.read_to_string(&mut str)?;

        Ok(toml::from_str(&str)?)
    }
}
