use anyhow::{anyhow, Result};

#[derive(Debug)]
pub struct ScriptCode {
    pub chain_id: u32,
    pub tx_type: u8,
    pub da_type: u8,
    pub version: u8,
    pub filling: u8,
    pub hash: Vec<u8>,
}

impl ScriptCode {
    pub fn decode(data: &[u8]) -> Result<Self> {
        if data.len() != 40 {
            return Err(anyhow!("Not long enough"));
        }

        let ret = Self {
            chain_id: u32::from_be_bytes([data[0], data[1], data[2], data[3]]),
            tx_type: data[4],
            da_type: data[5],
            version: data[6],
            filling: data[7],
            hash: data[8..].to_vec(),
        };
        log::info!("decode script code:{:#?}", ret);
        Ok(ret)
    }

    pub fn da_hash(&self) -> Vec<u8> {
        let mut hash = vec![self.da_type];
        hash.extend_from_slice(&self.hash);
        hash
    }

    pub fn encode(&self) -> [u8; 40] {
        let mut code: [u8; 40] = [0; 40];
        let chain_id = self.chain_id.to_be_bytes();
        code[..4].copy_from_slice(&chain_id[..4]);
        code[4] = self.tx_type;
        code[5] = self.da_type;
        code[6] = self.filling;
        code[8..(self.hash.len() + 8)].copy_from_slice(&self.hash);
        code
    }

    pub fn check(&self, chain_id: u32, da_tys: Vec<u8>) -> Result<()> {
        if 1 != self.tx_type && self.chain_id != chain_id {
            Err(anyhow!("chain id error:{} {}", self.chain_id, chain_id))
        } else if 0 != self.tx_type && 1 != self.tx_type {
            Err(anyhow!("tx type error:{}", self.tx_type))
        } else if !da_tys.contains(&self.da_type) {
            Err(anyhow!("da type error:{:?},{}", da_tys, self.da_type))
        } else if 0 != self.version {
            Err(anyhow!("version error:{}", self.version))
        } else if 0 != self.filling {
            Err(anyhow!("filling error:{}", self.filling))
        } else {
            Ok(())
        }
    }
}
