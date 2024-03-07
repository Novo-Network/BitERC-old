use std::{str::FromStr, sync::Arc};

use anyhow::Result;
use async_trait::async_trait;
use bitcoin::{consensus::deserialize, Address, Amount, Network, Transaction};
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
    fee_address: Address,
    da_fee: Amount,
    network: Network,
}

impl NovoHandle {
    pub fn new(
        da_mgr: Arc<DAServiceManager>,
        client: Arc<Client>,
        da_fee: u64,
        fee_address: &str,
        network: &str,
    ) -> Result<Self> {
        let fee_address = Address::from_str(fee_address).map(|addr| addr.assume_checked())?;
        let da_fee = Amount::from_sat(da_fee);
        let network = Network::from_str(network)?;
        Ok(Self {
            da_mgr,
            client,
            fee_address,
            da_fee,
            network,
        })
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

                let mut flag = false;
                for txout in tx.output.iter() {
                    if Ok(self.fee_address.clone())
                        == Address::from_script(&txout.script_pubkey, self.network)
                        && txout.value >= self.da_fee
                    {
                        flag = true;
                    };
                }
                if !flag {
                    return Err(RPCError::internal_error("da fee not found".to_string()));
                }

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
