use std::{str::FromStr, sync::Arc};

use anyhow::{anyhow, Result};
use bitcoin::{
    absolute::LockTime,
    ecdsa::Signature,
    hashes::Hash,
    opcodes::all::OP_RETURN,
    script::Builder,
    secp256k1::{Message, Secp256k1},
    sighash::SighashCache,
    transaction::Version,
    Address, Amount, EcdsaSighashType, OutPoint, PrivateKey, Script, ScriptBuf, Sequence,
    Transaction, TxIn, TxOut, Txid, Witness,
};
use bitcoincore_rpc::{
    json::SignRawTransactionInput, jsonrpc::serde_json::Value, Client as BitcoincoreClient, RpcApi,
};
use electrum_client::{Client as ElectrumClient, ElectrumApi, ListUnspentRes};
use ethers::{types::H160, utils::keccak256};
use json_rpc_server::call;

pub struct BtcTransactionBuilder {
    electrum_client: ElectrumClient,
    pub bitcoincore_client: Arc<BitcoincoreClient>,
}

impl BtcTransactionBuilder {
    pub fn new(electrs_url: &str, bitcoincore_client: Arc<BitcoincoreClient>) -> Result<Self> {
        Ok(Self {
            electrum_client: ElectrumClient::new(electrs_url)?,
            bitcoincore_client,
        })
    }
    pub fn get_eth_from_address(&self, txid: &Txid, vout: u32) -> Result<H160> {
        let script = self
            .bitcoincore_client
            .get_raw_transaction(txid, None)
            .map_err(|e| anyhow!(e))
            .and_then(|tx| {
                tx.output
                    .get(vout as usize)
                    .map(|v| v.script_pubkey.clone())
                    .ok_or(anyhow!("utxo not fount {:?}", txid))
            })?;

        let hash = match script.p2pk_public_key() {
            Some(v) => keccak256(keccak256(v.to_bytes())).to_vec(),
            None => script.script_hash().as_byte_array().to_vec(),
        };

        Ok(H160::from_slice(&hash[0..20]))
    }

    pub fn list_unspent(&self, script: &Script) -> Result<Vec<ListUnspentRes>> {
        self.electrum_client
            .script_list_unspent(script)
            .map_err(|e| anyhow!(e))
    }

    pub async fn build_transaction(
        &self,
        novo_api_url: &str,
        private_key: PrivateKey,
        script: ScriptBuf,
        unspents: Vec<ListUnspentRes>,
        eth_fee: u64,
        hash: &[u8; 40],
        pay_da_fee: bool,
    ) -> Result<Transaction> {
        log::info!("unspent:{:#?}", unspents);

        let mut fee = Amount::from_sat(eth_fee);

        let relay_fee = Amount::from_btc(self.electrum_client.relay_fee()?)?;
        log::info!("relay_fee:{:#?}", relay_fee);
        if fee < relay_fee {
            fee = relay_fee;
        }

        let (da_address, da_fee) = if pay_da_fee {
            let da_info = call::<Option<Value>, Value>(novo_api_url, "novo_getDaInfo", &None, None)
                .await
                .map_err(|e| anyhow!("{:?}", e))?
                .ok_or(anyhow!("da info empty"))?;

            let da_fee = da_info
                .get("fee")
                .and_then(|v| v.as_u64())
                .ok_or(anyhow!("da fee empty"))?;

            let addr = da_info
                .get("address")
                .and_then(|v| v.as_str())
                .ok_or(anyhow!("da address empty"))?;
            (
                Some(Address::from_str(addr).map(|a| a.assume_checked())?),
                Amount::from_sat(da_fee),
            )
        } else {
            (None, Amount::ZERO)
        };
        fee += da_fee;

        let mut input = Vec::new();
        let mut sign_inputs = Vec::new();
        let mut sum_amount = Amount::ZERO;
        for it in unspents.iter() {
            if Amount::from_sat(it.value) < fee {
                continue;
            }
            sum_amount += Amount::from_sat(it.value);

            let txin = TxIn {
                previous_output: OutPoint {
                    txid: it.tx_hash,
                    vout: it.tx_pos as u32,
                },
                sequence: Sequence::MAX,
                script_sig: ScriptBuf::default(),
                witness: Witness::new(),
            };
            input.push(txin);

            let sign_input = SignRawTransactionInput {
                txid: it.tx_hash,
                vout: it.tx_pos as u32,
                script_pub_key: script.clone(),
                redeem_script: None,
                amount: Some(Amount::from_sat(it.value)),
            };
            sign_inputs.push(sign_input);

            if sum_amount > fee {
                break;
            }
        }

        if sum_amount <= fee {
            return Err(anyhow!("Insufficient balance"));
        }
        let mut output = vec![
            TxOut {
                value: Amount::from_sat(0),
                script_pubkey: Builder::new()
                    .push_opcode(OP_RETURN)
                    .push_slice(hash)
                    .into_script(),
            },
            TxOut {
                value: sum_amount - fee,
                script_pubkey: script,
            },
        ];
        if let Some(addr) = da_address {
            output.push(TxOut {
                value: da_fee,
                script_pubkey: addr.script_pubkey(),
            })
        }
        // create transaction
        let mut unsigned_tx = Transaction {
            version: Version::ONE,
            lock_time: LockTime::ZERO,
            input,
            output,
        };
        let sighash_type = EcdsaSighashType::All;
        let secp = Secp256k1::new();

        let pk = private_key.public_key(&secp);
        let mut sighasher = SighashCache::new(&mut unsigned_tx);

        for (index, input) in sign_inputs.iter().enumerate() {
            let sighash = sighasher.p2wpkh_signature_hash(
                index,
                &input.script_pub_key,
                sum_amount,
                sighash_type,
            )?;

            // Sign the sighash using the secp256k1 library (exported by rust-bitcoin).
            let msg = Message::from(sighash);
            let signature = Signature {
                sig: secp.sign_ecdsa(&msg, &private_key.inner),
                hash_ty: sighash_type,
            };

            // Update the witness stack.
            let witness = sighasher
                .witness_mut(index)
                .ok_or(anyhow!("{} witness is none", index))?;
            *witness = Witness::p2wpkh(&signature, &pk.inner);
        }

        // Get the signed transaction.
        Ok(sighasher.into_transaction().clone())
    }
}
