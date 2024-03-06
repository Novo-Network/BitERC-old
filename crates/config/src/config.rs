use std::{fs::File, io::Read};

use anyhow::Result;
#[cfg(feature = "celestia")]
use da::CelestiaConfig;
use da::DaType;
#[cfg(feature = "file")]
use da::FileConfig;
#[cfg(feature = "greenfield")]
use da::GreenfieldConfig;
#[cfg(feature = "ipfs")]
use da::IpfsConfig;
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

    #[cfg(feature = "file")]
    pub file: Option<FileConfig>,
    #[cfg(feature = "ipfs")]
    pub ipfs: Option<IpfsConfig>,
    #[cfg(feature = "celestia")]
    pub celestia: Option<CelestiaConfig>,
    #[cfg(feature = "greenfield")]
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
