use std::str::FromStr;

use anyhow::{anyhow, Error, Result};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Eq, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum DaType {
    File,
    Ipfs,
    Celestia,
    Greenfield,
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
            "file" => Ok(Self::File),
            "ipfs" => Ok(DaType::Ipfs),
            "celestia" => Ok(DaType::Celestia),
            "greenfield" => Ok(DaType::Greenfield),
            &_ => Err(anyhow!("default can only be file ipfs celestia")),
        }
    }
}
