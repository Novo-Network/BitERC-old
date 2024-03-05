use std::{fs, path::PathBuf};

use anyhow::{Ok, Result};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use sha3::{Digest, Keccak256};

use crate::{service::DAService, DaType};

#[derive(Debug, Deserialize, Serialize)]
pub struct Config {
    pub path: String,
}

pub struct FileService {
    path: PathBuf,
}

impl FileService {
    pub fn new(cfg: Config) -> Result<Self> {
        let path: PathBuf = cfg.path.into();
        if !path.exists() {
            fs::create_dir_all(&path)?;
        }
        Ok(Self { path })
    }

    pub fn hash(tx: &[u8]) -> Vec<u8> {
        Keccak256::digest(tx).to_vec()
    }
}

#[async_trait]
impl DAService for FileService {
    async fn set_full_tx(&self, tx: &[u8]) -> Result<Vec<u8>> {
        let hash = Self::hash(tx);
        let key = hex::encode(&hash);
        let path = self.path.join(key);
        let value = hex::encode(tx);

        fs::write(path, value)?;

        Ok(hash)
    }

    async fn get_tx(&self, hash: &[u8]) -> Result<Vec<u8>> {
        let key = hex::encode(hash);
        let path = self.path.join(key);

        let content = if path.exists() {
            let file_content = fs::read_to_string(path)?;
            hex::decode(file_content)?
        } else {
            vec![]
        };

        Ok(content)
    }

    fn type_byte(&self) -> u8 {
        DaType::File.type_byte()
    }
}
