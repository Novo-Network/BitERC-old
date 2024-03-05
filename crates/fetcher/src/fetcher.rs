use std::sync::Arc;

use anyhow::{anyhow, Result};
use bitcoin::{
    hashes::Hash,
    opcodes::all::{OP_PUSHBYTES_40, OP_RETURN},
    Block, Transaction, TxOut,
};
use bitcoincore_rpc::{Client, RpcApi};
use da::DAServiceManager;
use rt_evm::model::{
    codec::ProtocolCodec,
    types::{SignedTransaction, UnsignedTransaction, UnverifiedTransaction, H160, H256, U256},
};
use tx_builder::{btc::BtcTransactionBuilder, SAT2WEI};
use utils::ScriptCode;

pub struct Fetcher {
    height: u64,
    builder: BtcTransactionBuilder,
    chain_id: u32,
    da_mgr: Arc<DAServiceManager>,
    client: Arc<Client>,
}

impl Fetcher {
    pub async fn new(
        electrs_url: &str,
        client: Arc<Client>,
        start: u64,
        chain_id: u32,
        da_mgr: Arc<DAServiceManager>,
    ) -> Result<Self> {
        let block_cnt = client.clone().get_block_count()?;
        if start > block_cnt + 1 {
            return Err(anyhow!(
                "The starting height is greater than the chain height"
            ));
        }

        Ok(Self {
            height: start,
            builder: BtcTransactionBuilder::new(electrs_url, client.clone())?,
            chain_id,
            da_mgr,
            client,
        })
    }

    pub async fn get_block(&mut self) -> Result<Option<Block>> {
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

        self.height += 1;
        Ok(Some(block))
    }

    fn verify_transaction(&self, btc_tx: &Transaction) -> Result<u64> {
        if btc_tx.input.is_empty() {
            return Err(anyhow!("tx input is empty"));
        }

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

        let output_amuont = btc_tx
            .output
            .iter()
            .map(|txout| txout.value.to_sat())
            .sum::<u64>();

        if input_amount > output_amuont {
            Ok(input_amount - output_amuont)
        } else {
            Err(anyhow!("Input amount does not match output amount"))
        }
    }

    pub async fn decode_transaction(&self, btc_tx: &Transaction) -> Result<Vec<SignedTransaction>> {
        let source_hash = H256::from(btc_tx.txid().to_byte_array());
        let from = if let Some(txin) = btc_tx.input.first() {
            self.builder
                .get_eth_from_address(&txin.previous_output.txid, txin.previous_output.vout)?
        } else {
            return Err(anyhow!("input not found"));
        };
        let fee = U256::from(self.verify_transaction(btc_tx)?);

        let mut ret = vec![];
        for (index, out) in btc_tx.output.iter().enumerate() {
            match self.decode_vout(out, source_hash, from).await {
                Ok(tx) => {
                    if fee >= (tx.transaction.unsigned.gas_limit() / U256::from(SAT2WEI)) {
                        ret.push(tx)
                    };
                }
                Err(e) => {
                    log::debug!("decode {} vout {} error:{}", btc_tx.txid(), index, e);
                }
            }
        }
        Ok(ret)
    }

    async fn decode_vout(
        &self,
        out: &TxOut,
        source_hash: H256,
        sender: H160,
    ) -> Result<SignedTransaction> {
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
        let mut transaction =
            UnverifiedTransaction::decode(&tx_data).map_err(|e| anyhow!(e.to_string()))?;

        if let UnsignedTransaction::Deposit(ref mut tx) = transaction.unsigned {
            tx.source_hash = source_hash;
            tx.from = sender;
        };

        transaction.chain_id = self.chain_id.into();
        transaction.hash = transaction.get_hash();

        let tx = SignedTransaction {
            transaction,
            sender,
            public: None,
        };
        Ok(tx)
    }
}
