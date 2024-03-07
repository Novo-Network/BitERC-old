use std::sync::Arc;

use async_trait::async_trait;
use bitcoin::{consensus::deserialize, Transaction};
use bitcoincore_rpc::{
    jsonrpc::serde_json::{json, Value},
    Client, RpcApi,
};
use da::DAServiceManager;
use ethers::types::Bytes;
use json_rpc_server::{Handle, RPCError, RPCResult};
use serde::{Deserialize, Serialize};

pub struct NovoHandle {
    da_mgr: Arc<DAServiceManager>,
    client: Arc<Client>,
    da_fee: u64,
    fee_address: String,
}

impl NovoHandle {
    pub fn new(
        da_mgr: Arc<DAServiceManager>,
        client: Arc<Client>,
        da_fee: u64,
        fee_address: &str,
    ) -> Self {
        Self {
            da_mgr,
            client,
            da_fee,
            fee_address: fee_address.into(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum NovoHandleRequest {
    SendRawTransactionArray((Bytes, Bytes)),
    SendRawTransaction { tx_data: Bytes, btc_tx: Bytes },
    GetDaINfo,
}

impl NovoHandleRequest {
    pub fn into_send_raw_transaction(self) -> RPCResult<(Bytes, Bytes)> {
        match self {
            Self::SendRawTransactionArray(s) => Ok(s),
            Self::SendRawTransaction { tx_data, btc_tx } => Ok((tx_data, btc_tx)),
            _ => Err(RPCError::invalid_params()),
        }
    }
}

#[async_trait]
impl Handle for NovoHandle {
    type Request = NovoHandleRequest;
    type Response = Value;

    async fn handle(
        &self,
        method: &str,
        req: Option<Self::Request>,
    ) -> std::result::Result<Option<Self::Response>, RPCError> {
        match method {
            "novo_sendRawTransaction" => {
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

                Ok(Some(Value::String(format!("{}", txid))))
            }
            "novo_getDaInfo" => Ok(Some(json!({
                "address": &self.fee_address,
                "fee": self.da_fee,

            }))),
            _ => Err(RPCError::unknown_method()),
        }
    }
}
