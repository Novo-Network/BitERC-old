use std::sync::Arc;

use async_trait::async_trait;
use bitcoin::{consensus::deserialize, Transaction};
use bitcoincore_rpc::{Client, RpcApi};
use da::DAServiceManager;
use ethers::types::Bytes;
use json_rpc_server::{Handle, RPCError, RPCResult};
use serde::{Deserialize, Serialize};

pub struct ApiHandle {
    da_mgr: Arc<DAServiceManager>,
    client: Arc<Client>,
    da_fee: u64,
}

impl ApiHandle {
    pub fn new(da_mgr: Arc<DAServiceManager>, client: Arc<Client>, da_fee: u64) -> Self {
        Self {
            da_mgr,
            client,
            da_fee,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ApiHandleRequest {
    SendRawTransaction((Bytes, Bytes)),
    GetDaFee,
}
macro_rules! define_into {
    ($func: ident, $ret: ty, $e: ident) => {
        pub fn $func(self) -> RPCResult<$ret> {
            match self {
                Self::$e(v) => Ok(v),
                _ => Err(RPCError::invalid_params()),
            }
        }
    };
}

impl ApiHandleRequest {
    define_into!(
        into_send_raw_transaction,
        (Bytes, Bytes),
        SendRawTransaction
    );
}

#[async_trait]
impl Handle for ApiHandle {
    type Request = ApiHandleRequest;
    type Response = String;

    async fn handle(
        &self,
        method: &str,
        req: Option<Self::Request>,
    ) -> std::result::Result<Option<Self::Response>, RPCError> {
        match method {
            "api_sendRawTransaction" => {
                let (eth_tx_bytes, btc_tx_bytes) = req
                    .ok_or(RPCError::invalid_params())?
                    .into_send_raw_transaction()?;
                log::info!("send raw eth tx: {}", eth_tx_bytes);
                log::info!("send raw btc tx: {}", btc_tx_bytes);

                self.da_mgr
                    .set_tx(&eth_tx_bytes)
                    .await
                    .map_err(|e| RPCError::internal_error(e.to_string()))?;

                let tx: Transaction = deserialize(&btc_tx_bytes)
                    .map_err(|e| RPCError::internal_error(e.to_string()))?;
                let txid = self
                    .client
                    .send_raw_transaction(&tx)
                    .map_err(|e| RPCError::internal_error(e.to_string()))?;

                Ok(Some(format!("{}", txid)))
            }
            "api_getDaFee" => Ok(Some(format!("{}", self.da_fee))),
            _ => Err(RPCError::unknown_method()),
        }
    }
}
