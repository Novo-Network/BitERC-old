use std::str::FromStr;

use anyhow::{anyhow, Error, Result};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Eq, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum DaType {
    #[cfg(feature = "file")]
    File = 0,
    #[cfg(feature = "ipfs")]
    Ipfs = 1,
    #[cfg(feature = "celestia")]
    Celestia = 2,
    #[cfg(feature = "greenfield")]
    Greenfield = 3,
}
impl DaType {
    pub fn type_byte(&self) -> u8 {
        self.clone() as u8
    }
}
impl FromStr for DaType {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self> {
        match s {
            #[cfg(feature = "file")]
            "file" => Ok(Self::File),
            #[cfg(feature = "ipfs")]
            "ipfs" => Ok(DaType::Ipfs),
            #[cfg(feature = "celestia")]
            "celestia" => Ok(DaType::Celestia),
            #[cfg(feature = "greenfield")]
            "greenfield" => Ok(DaType::Greenfield),
            &_ => Err(anyhow!("da type error")),
        }
    }
}
