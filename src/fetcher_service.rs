use std::sync::Arc;

use bitcoin::{
    opcodes::all::{OP_PUSHBYTES_40, OP_RETURN},
    Block, Transaction, TxOut,
};
use bitcoincore_rpc::{Auth, Client, RpcApi};
use da::DAServiceManager;
use ethers::{types::transaction::eip2718::TypedTransaction, utils::rlp::Rlp};
use rt_evm_model::{
    codec::hex_encode,
    types::{
        LegacyTransaction, SignatureComponents, SignedTransaction, TransactionAction,
        UnsignedTransaction, UnverifiedTransaction, U256,
    },
};
use ruc::*;

use crate::{tx::SAT2WEI, vout_code::VoutCode};

pub struct FetcherService {
    height: u64,
    cli: Client,
    chain_id: u32,
    da_mgr: Arc<DAServiceManager>,
}

impl FetcherService {
    pub fn new(
        btc_url: &str,
        username: &str,
        password: &str,
        start: u64,
        chain_id: u32,
        da_mgr: Arc<DAServiceManager>,
    ) -> Result<Self> {
        let cli = Client::new(
            btc_url,
            Auth::UserPass(username.to_string(), password.to_string()),
        )
        .map_err(|e| eg!(e))?;
        let block_cnt = cli.get_block_count().c(d!())?;
        if start > block_cnt + 1 {
            return Err(eg!("The starting height is greater than the chain height"));
        }

        Ok(Self {
            height: start,
            cli,
            chain_id,
            da_mgr,
        })
    }
    pub async fn get_block(&mut self) -> Result<Option<Block>> {
        let block_cnt = self.cli.get_block_count().c(d!())?;
        if self.height > block_cnt {
            return Ok(None);
        }
        let hash = self.cli.get_block_hash(self.height).c(d!())?;
        let block = self.cli.get_block(&hash).c(d!())?;
        log::info!(
            "get {} block:{},{:#?}",
            self.height,
            block.block_hash(),
            block
        );

        self.height += 1;
        Ok(Some(block))
    }
    pub async fn decode_transaction(&self, btc_tx: &Transaction) -> Result<Vec<SignedTransaction>> {
        let mut ret = vec![];
        let input_amount = btc_tx
            .input
            .iter()
            .map(|txin| {
                self.cli
                    .get_raw_transaction(&txin.previous_output.txid, None)
                    .c(d!())
                    .and_then(|tx| {
                        tx.output
                            .get(txin.previous_output.vout as usize)
                            .map(|v| v.value)
                            .ok_or(eg!("utxo not fount {:?}", txin.previous_output))
                    })
            })
            .collect::<Result<Vec<_>>>()?
            .iter()
            .map(|v| v.to_sat())
            .sum::<u64>();
        let output_amuont = btc_tx
            .output
            .iter()
            .map(|txout| txout.value.to_sat())
            .sum::<u64>();
        if input_amount <= output_amuont {
            Ok(ret)
        } else if btc_tx.input.is_empty() {
            Ok(ret)
        } else {
            for (index, out) in btc_tx.output.iter().enumerate() {
                match self.decode_vout(out).await {
                    Ok(tx) => {
                        if U256::from(input_amount - output_amuont)
                            >= (tx.transaction.unsigned.gas_limit() / U256::from(SAT2WEI))
                        {
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
    }

    async fn decode_vout(&self, out: &TxOut) -> Result<SignedTransaction> {
        let code = out.script_pubkey.as_bytes();
        if code.len() != 42
            || Some(OP_RETURN) != code.first().cloned().map(From::from)
            || Some(OP_PUSHBYTES_40) != code.get(1).cloned().map(From::from)
        {
            return Err(eg!(
                "code.len():{},first:{:?},last:{:?}",
                code.len(),
                code.first(),
                code.last()
            ));
        }
        let vc = VoutCode::decode(&code[2..]).c(d!())?;
        log::debug!("decode_vout:{}:{:?}", hex_encode(code), vc);
        vc.check(self.chain_id, self.da_mgr.types()).c(d!())?;
        let da_hash = vc.da_hash();
        log::debug!("da hash:{}", hex_encode(&da_hash));
        let tx_data = self.da_mgr.get_tx(da_hash).await.map_err(|e| eg!(e))?;
        log::debug!("tx_data:{}", hex_encode(&tx_data));
        let (evm_tx, _sign) =
            TypedTransaction::decode_signed(&Rlp::new(&tx_data)).map_err(|e| eg!(e))?;

        let action = match evm_tx.to() {
            Some(addr) => TransactionAction::Call(addr.as_address().cloned().c(d!())?),
            None => TransactionAction::Create,
        };

        let transaction = UnverifiedTransaction {
            unsigned: UnsignedTransaction::Legacy(LegacyTransaction {
                nonce: evm_tx.nonce().cloned().c(d!())?,
                gas_price: evm_tx.gas_price().c(d!())?,
                gas_limit: evm_tx.gas().cloned().c(d!())?,
                action,
                value: evm_tx.value().cloned().c(d!())?,
                data: match evm_tx.data().cloned() {
                    Some(v) => v.to_vec(),
                    None => Vec::new(),
                },
            }),
            signature: Some(SignatureComponents::from(_sign.to_vec())),
            chain_id: evm_tx.chain_id().c(d!())?.as_u64(),
            hash: evm_tx.hash(&_sign),
        };

        let tx = SignedTransaction {
            transaction: transaction.calc_hash(),
            sender: evm_tx.from().cloned().c(d!())?,
            public: None,
        };
        Ok(tx)
    }
}
