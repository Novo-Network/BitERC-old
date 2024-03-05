use std::collections::BTreeMap;

use ethers::types::{Bytes, H160, H256, U256};
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize)]
pub struct Account {
    pub balance: Option<U256>,
    pub nonce: Option<U256>,
    pub code: Option<Bytes>,
    pub storage: Option<BTreeMap<U256, U256>>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct ChainConfig {
    pub chain_id: u32,
    pub bin_hash: H256,
    pub accounts: BTreeMap<H160, Account>,
}
