use std::str::FromStr;

use bitcoin::{
    absolute::LockTime,
    ecdsa::Signature,
    hashes::{sha256, Hash},
    opcodes::all::OP_RETURN,
    script::Builder,
    secp256k1::{All, Message, Secp256k1, SecretKey},
    sighash::SighashCache,
    transaction::Version,
    Address, Amount, EcdsaSighashType, Network, OutPoint, PrivateKey, ScriptBuf, Sequence,
    Transaction, TxIn, TxOut, Txid, Witness,
};
use bitcoincore_rpc::{json::SignRawTransactionInput, Auth, Client as BitcoincoreClient, RpcApi};
use electrum_client::{Client as ElectrumClient, ElectrumApi};
use ethers::{types::H160, utils::keccak256};
use ruc::*;

#[allow(unused)]
pub struct BtcTransactionBuilder {
    electrum_client: ElectrumClient,
    pub bitcoincore_client: BitcoincoreClient,
}

#[allow(unused)]
impl BtcTransactionBuilder {
    pub async fn new(
        electrs_url: &str,
        btc_url: &str,
        username: &str,
        password: &str,
    ) -> Result<Self> {
        let electrum_client = ElectrumClient::new(electrs_url).c(d!())?;
        let bitcoincore_client = BitcoincoreClient::new(
            btc_url,
            Auth::UserPass(username.to_string(), password.to_string()),
        )
        .c(d!())?;
        Ok(Self {
            electrum_client,
            bitcoincore_client,
        })
    }
    pub async fn get_eth_from_address(&self, txid: &Txid, vout: u32) -> Result<H160> {
        let script = self
            .bitcoincore_client
            .get_raw_transaction(txid, None)
            .c(d!())
            .and_then(|tx| {
                tx.output
                    .get(vout as usize)
                    .map(|v| v.script_pubkey.clone())
                    .ok_or(eg!("utxo not fount {:?}", txid))
            })?;

        let hash = if script.is_p2pk() || script.is_p2pkh() {
            let data = script.p2pk_public_key().c(d!())?.to_bytes();
            keccak256(keccak256(data))
        } else {
            let mut hasher = sha256::HashEngine::default();
            let data = script.as_bytes().to_vec();
            sha256::Hash::from_engine(hasher).to_byte_array()
        };
        Ok(H160::from_slice(&hash[0..20]))
    }
    pub async fn build_transaction(
        &self,
        sk: &str,
        network: &str,
        address: &str,
        eth_fee: u64,
        hash: &[u8; 40],
    ) -> Result<Txid> {
        let private_key = PrivateKey {
            compressed: true,
            network: Network::from_core_arg(network).c(d!())?,
            inner: SecretKey::from_str(sk.strip_prefix("0x").unwrap_or(sk)).c(d!())?,
        };
        let mut fee = Amount::from_sat(eth_fee);
        let relay_fee = Amount::from_btc(self.electrum_client.relay_fee().c(d!())?).c(d!())?;
        if fee < relay_fee {
            fee = relay_fee;
        }
        let secp: Secp256k1<All> = Secp256k1::new();
        let pk = private_key.public_key(&secp);
        let addr = Address::from_str(address)
            .map(|addr| addr.assume_checked())
            .c(d!())?;
        let mut input = Vec::new();
        let mut sign_inputs = Vec::new();
        let mut sum_amount = Amount::ZERO;

        let unspents = self
            .electrum_client
            .script_list_unspent(&addr.script_pubkey())
            .c(d!())?;

        log::info!("unspent:{:#?}", unspents);
        for it in unspents.iter() {
            sum_amount += Amount::from_sat(it.value);
            let txin = TxIn {
                previous_output: OutPoint {
                    txid: it.tx_hash,
                    vout: it.tx_pos as u32,
                },
                sequence: Sequence::MAX,
                script_sig: ScriptBuf::new(),
                witness: Witness::new(),
            };
            input.push(txin);
            let sign_input = SignRawTransactionInput {
                txid: it.tx_hash,
                vout: it.tx_pos as u32,
                script_pub_key: addr.script_pubkey(),
                redeem_script: None,
                amount: Some(Amount::from_sat(it.value)),
            };

            sign_inputs.push(sign_input);
        }

        if sum_amount <= fee {
            return Err(eg!("Insufficient balance"));
        }

        // create transaction
        let mut unsigned_tx = Transaction {
            version: Version::ONE,
            lock_time: LockTime::ZERO,
            input,
            output: vec![
                TxOut {
                    value: Amount::from_sat(0),
                    script_pubkey: Builder::new()
                        .push_opcode(OP_RETURN)
                        .push_slice(hash)
                        .into_script(),
                },
                TxOut {
                    value: sum_amount - fee,
                    script_pubkey: addr.script_pubkey(),
                },
            ],
        };
        let sighash_type = EcdsaSighashType::All;
        let mut sighasher = SighashCache::new(&mut unsigned_tx);
        let sighash = sighasher
            .p2wpkh_signature_hash(
                0,
                &sign_inputs
                    .first()
                    .map(|v| v.script_pub_key.clone())
                    .c(d!())?,
                sum_amount,
                sighash_type,
            )
            .c(d!())?;

        let secp = Secp256k1::new();
        // Sign the sighash using the secp256k1 library (exported by rust-bitcoin).
        let msg = Message::from(sighash);

        let signature = secp.sign_ecdsa(&msg, &private_key.inner);

        // Update the witness stack.
        let signature = Signature {
            sig: signature,
            hash_ty: sighash_type,
        };

        *sighasher.witness_mut(0).unwrap() = Witness::p2wpkh(&signature, &pk.inner);

        // Get the signed transaction.
        let tx = sighasher.into_transaction().clone();
        log::info!("btc tx:{:#?}", tx);
        self.bitcoincore_client.send_raw_transaction(&tx).c(d!())
    }
}
