#![deny(warnings, unused_crate_dependencies)]

mod service;
pub use service::*;

mod file_service;
pub use file_service::*;

mod ipfs_service;
pub use ipfs_service::*;

mod celestia_service;
pub use celestia_service::*;

mod greenfield_servic;
pub use greenfield_servic::*;

use std::str::FromStr;

use anyhow::{anyhow, Error, Result};

#[derive(Clone, Eq, PartialEq)]
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

#[allow(clippy::too_many_arguments)]
pub async fn create_da_mgr(
    file: bool,
    file_path: Option<&str>,
    ipfs: bool,
    ipfs_url: Option<&str>,
    celestia: bool,
    celestia_url: Option<&str>,
    celestia_token: Option<&str>,
    celestia_namespace_id: Option<&str>,
    greenfield: bool,
    greenfield_rpc_addr: Option<&str>,
    greenfield_chain_id: Option<&str>,
    greenfield_bucket: Option<&str>,
    greenfield_password_file: Option<&str>,
    default: DaType,
) -> Result<DAServiceManager> {
    match default {
        DaType::File => {
            if !file {
                return Err(anyhow!("file flag not enabled"));
            }
        }
        DaType::Ipfs => {
            if !ipfs {
                return Err(anyhow!("ipfs flag not enabled"));
            }
        }
        DaType::Celestia => {
            if !celestia {
                return Err(anyhow!("celestia flag not enabled"));
            }
        }
        DaType::Greenfield => {
            if !greenfield {
                return Err(anyhow!("celestia flag not enabled"));
            }
        }
    }

    let mut da_mgr = DAServiceManager::new();
    if file {
        let file_path = file_path.ok_or(anyhow!("file path can not be empty"))?;
        let file_service = FileService::new(file_path)?;
        if DaType::File == default {
            da_mgr.add_default_service(file_service);
        } else {
            da_mgr.add_service(file_service);
        }
    }

    if ipfs {
        let ipfs_url = ipfs_url.ok_or(anyhow!("ipfs url can not be empty"))?;
        let ipfs_service = IpfsService::new(ipfs_url)?;
        if DaType::Ipfs == default {
            da_mgr.add_default_service(ipfs_service);
        } else {
            da_mgr.add_service(ipfs_service);
        }
    }

    if celestia {
        let celestia_url = celestia_url.ok_or(anyhow!("celestia url can not be empty"))?;
        let namespace_id = celestia_namespace_id
            .ok_or(anyhow!("namespace id can not be empty"))
            .and_then(|v| hex::decode(v).map_err(|e| anyhow!("{}", e)))
            .and_then(|v| {
                v.try_into()
                    .map_err(|_e| anyhow!("namespace try into error"))
            })?;
        let celestia_service =
            CelestiaService::new(celestia_url, celestia_token, namespace_id).await?;
        if DaType::Celestia == default {
            da_mgr.add_default_service(celestia_service);
        } else {
            da_mgr.add_service(celestia_service);
        }
    }

    if greenfield {
        let rpc_addr =
            greenfield_rpc_addr.ok_or(anyhow!("greenfield rpc addr can not be empty"))?;
        let chain_id =
            greenfield_chain_id.ok_or(anyhow!("greenfield chain id can not be empty"))?;
        let bucket = greenfield_bucket.ok_or(anyhow!("greenfield bucket can not be empty"))?;
        let password_file =
            greenfield_password_file.ok_or(anyhow!("greenfield bucket can not be empty"))?;

        let greenfield_service = GreenfieldService::new(
            rpc_addr.into(),
            chain_id.into(),
            bucket.into(),
            password_file.into(),
        );
        if DaType::Greenfield == default {
            da_mgr.add_default_service(greenfield_service);
        } else {
            da_mgr.add_service(greenfield_service);
        }
    }
    Ok(da_mgr)
}
