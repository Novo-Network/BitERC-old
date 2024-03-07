use std::{str::FromStr, sync::Arc};

use anyhow::{anyhow, Result};
use bitcoin::{
    hashes::Hash,
    opcodes::all::{OP_PUSHBYTES_40, OP_RETURN},
    Address, Amount, Block, Network, Transaction, TxOut, Txid,
};
use bitcoincore_rpc::{Client, RpcApi};
use config::ChainConfig;
use da::DAServiceManager;
use ethers::utils::rlp::Rlp;
use rt_evm::model::types::{DepositTransaction, SignedTransaction, H160, H256, U256};
use tx_builder::{btc::BtcTransactionBuilder, SAT2WEI};
use utils::ScriptCode;

pub enum Data {
    Config(ChainConfig),
    Transaction(SignedTransaction),
}
pub struct Fetcher {
    height: u64,
    builder: BtcTransactionBuilder,
    pub chain_id: u32,
    da_mgr: Arc<DAServiceManager>,
    client: Arc<Client>,
    fee_address: Address,
    da_fee: Amount,
    network: Network,
}

impl Fetcher {
    pub async fn new(
        electrs_url: &str,
        client: Arc<Client>,
        start: u64,
        chain_id: u32,
        da_mgr: Arc<DAServiceManager>,
        fee_address: &str,
        da_fee: u64,
        network: &str,
    ) -> Result<Self> {
        let block_cnt = client.clone().get_block_count()?;
        if start > block_cnt + 1 {
            return Err(anyhow!(
                "The starting height is greater than the chain height"
            ));
        }
        let fee_address = Address::from_str(fee_address).map(|addr| addr.assume_checked())?;
        let da_fee = Amount::from_sat(da_fee);
        let network = Network::from_str(network)?;
        Ok(Self {
            height: start,
            builder: BtcTransactionBuilder::new(electrs_url, client.clone())?,
            chain_id,
            da_mgr,
            client,
            fee_address,
            da_fee,
            network,
        })
    }

    pub async fn fetcher_first_cfg(&mut self) -> Result<(u64, ChainConfig)> {
        loop {
            if let Some((_, datas)) = self.fetcher().await? {
                for data in datas {
                    if let Data::Config(cfg) = data {
                        return Ok((self.height, cfg));
                    }
                }
            }
        }
    }

    pub async fn fetcher(&mut self) -> Result<Option<(u64, Vec<Data>)>> {
        let block = if let Some(block) = self.get_block().await? {
            block
        } else {
            return Ok(None);
        };
        self.height += 1;

        let mut ret = vec![];
        for tx in block.txdata.iter() {
            if let Some(datas) = self.decode_data(tx).await? {
                ret.extend(datas);
            };
        }
        Ok(Some((block.header.time.into(), ret)))
    }

    async fn get_block(&self) -> Result<Option<Block>> {
        let block_cnt = self.client.get_block_count()?;
        if self.height > block_cnt {
            return Ok(None);
        }

        let hash = self.client.get_block_hash(self.height)?;
        let block = self.client.get_block(&hash)?;
        log::info!(
            "get {} block:{},{:#?}",
            self.height,
            block.block_hash(),
            block
        );

        Ok(Some(block))
    }

    fn verify_transaction(&self, btc_tx: &Transaction) -> Result<Option<u64>> {
        let mut input_amount = 0;
        for txin in btc_tx.input.iter() {
            let tx = self
                .client
                .get_raw_transaction(&txin.previous_output.txid, None)?;
            let value = tx
                .output
                .get(txin.previous_output.vout as usize)
                .map(|v| v.value)
                .ok_or(anyhow!("utxo not found {:?}", txin.previous_output))?
                .to_sat();
            input_amount += value;
        }

        let mut flag = false;
        let mut output_amuont = 0;
        for txout in btc_tx.output.iter() {
            output_amuont += txout.value.to_sat();
            if Ok(self.fee_address.clone())
                == Address::from_script(&txout.script_pubkey, self.network)
            {
                if txout.value >= self.da_fee {
                    flag = true;
                }
            };
        }
        if !flag {
            return Err(anyhow!("da fee not found"));
        }

        if input_amount > output_amuont {
            Ok(Some(input_amount - output_amuont))
        } else {
            Ok(None)
        }
    }

    async fn decode_data(&self, btc_tx: &Transaction) -> Result<Option<Vec<Data>>> {
        let source_hash = H256::from(btc_tx.txid().to_byte_array());

        let from = if let Some(txin) = btc_tx.input.first() {
            if txin.previous_output.txid != Txid::all_zeros() {
                self.builder
                    .get_eth_from_address(&txin.previous_output.txid, txin.previous_output.vout)?
            } else {
                return Ok(None);
            }
        } else {
            return Err(anyhow!("input not found"));
        };

        let mut ret = vec![];
        let fee = U256::from(match self.verify_transaction(btc_tx)? {
            Some(v) => v,
            None => return Ok(None),
        });

        for (index, out) in btc_tx.output.iter().enumerate() {
            let data = match self.decode_vout(out, source_hash, from).await {
                Ok(data) => data,
                Err(e) => {
                    log::debug!("decode {} vout {} error:{}", btc_tx.txid(), index, e);
                    continue;
                }
            };
            if let Data::Transaction(ref tx) = data {
                if fee >= (tx.transaction.unsigned.gas_limit() / U256::from(SAT2WEI)) {
                    ret.push(data)
                };
            } else {
                ret.push(data)
            }
        }

        Ok(Some(ret))
    }

    async fn decode_vout(&self, out: &TxOut, source_hash: H256, sender: H160) -> Result<Data> {
        let code = out.script_pubkey.as_bytes();
        if code.len() != 42
            || Some(OP_RETURN) != code.first().cloned().map(From::from)
            || Some(OP_PUSHBYTES_40) != code.get(1).cloned().map(From::from)
        {
            return Err(anyhow!(
                "code.len():{},first:{:?},last:{:?}",
                code.len(),
                code.first(),
                code.last()
            ));
        }
        let vc = ScriptCode::decode(&code[2..])?;
        log::debug!("decode_vout:{}:{:?}", hex::encode(code), vc);
        vc.check(self.chain_id, self.da_mgr.types())?;

        let da_hash = vc.da_hash();
        log::debug!("da hash:{}", hex::encode(&da_hash));

        let tx_data = self.da_mgr.get_tx(da_hash).await.map_err(|e| anyhow!(e))?;
        log::debug!("tx_data:{}", hex::encode(&tx_data));

        if vc.tx_type == 0 {
            if Some(0x7e) != tx_data.first().copied() {
                return Err(anyhow!("not a deposit transaction"));
            }
            let mut deposit_tx = DepositTransaction::decode(&Rlp::new(&tx_data[1..]))?;
            deposit_tx.from = sender;
            deposit_tx.source_hash = source_hash;

            let tx = SignedTransaction::from_deposit_tx(deposit_tx, self.chain_id.into());
            log::info!("transaction:{:#?}", tx);
            Ok(Data::Transaction(tx))
        } else if vc.tx_type == 1 {
            let cfg = serde_json::from_slice(&tx_data)?;
            log::info!("chain config:{:#?}", cfg);
            Ok(Data::Config(cfg))
        } else {
            Err(anyhow!("tx type error"))
        }
    }
}
