use std::str::FromStr;

use bitcoin::{
    absolute::LockTime,
    ecdsa::Signature,
    opcodes::all::OP_RETURN,
    script::Builder,
    secp256k1::{All, Message, Secp256k1, SecretKey},
    sighash::SighashCache,
    transaction::Version,
    Address, Amount, EcdsaSighashType, Network, OutPoint, PrivateKey, ScriptBuf, Sequence,
    Transaction, TxIn, TxOut, Txid, Witness,
};
use bitcoincore_rpc::{json, Auth, Client, RpcApi};
use ruc::*;

#[allow(unused)]
pub struct BtcTransactionBuilder {
    client: Client,
    private_key: PrivateKey,
}

#[allow(unused)]
impl BtcTransactionBuilder {
    pub async fn new(
        url: &str,
        username: &str,
        password: &str,
        sk: &str,
        network: &str,
    ) -> Result<Self> {
        let client = Client::new(
            url,
            Auth::UserPass(username.to_string(), password.to_string()),
        )
        .c(d!())?;

        let private_key = PrivateKey {
            compressed: true,
            network: Network::from_core_arg(network).c(d!())?,
            inner: SecretKey::from_str(sk.strip_prefix("0x").unwrap_or(sk)).c(d!())?,
        };

        Ok(Self {
            client,
            private_key,
        })
    }
    pub async fn build_transaction(
        &self,
        fee: u64,
        hash: &[u8; 40],
        txid: String,
        vout: u32,
    ) -> Result<Txid> {
        let fee = Amount::from_sat(fee);
        let secp: Secp256k1<All> = Secp256k1::new();
        let pk = self.private_key.public_key(&secp);
        let addr = Address::p2wpkh(&pk, self.private_key.network).c(d!())?;
        log::info!("btc from address: {}", addr);
        let mut input = Vec::new();
        let mut sign_inputs = Vec::new();
        let mut sum_amount = Amount::ZERO;

        let txid = Txid::from_str(&txid).c(d!())?;
        let unspent = self.client.get_tx_out(&txid, vout, None).c(d!())?.c(d!())?;
        log::info!("unspent:{:#?}", unspent);
        sum_amount += unspent.value;
        let txin = TxIn {
            previous_output: OutPoint { txid, vout },
            sequence: Sequence::MAX,
            script_sig: ScriptBuf::new(),
            witness: Witness::new(),
        };
        input.push(txin);
        let sign_input = json::SignRawTransactionInput {
            txid,
            vout,
            script_pub_key: unspent.script_pub_key.script().c(d!())?,
            redeem_script: None,
            amount: Some(unspent.value),
        };

        sign_inputs.push(sign_input);

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
                    value: (sum_amount - fee),
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

        let signature = secp.sign_ecdsa(&msg, &self.private_key.inner);

        // Update the witness stack.
        let signature = Signature {
            sig: signature,
            hash_ty: sighash_type,
        };

        *sighasher.witness_mut(0).unwrap() = Witness::p2wpkh(&signature, &pk.inner);

        // Get the signed transaction.
        let tx = sighasher.into_transaction().clone();
        log::info!("btc tx:{:#?}", tx);
        self.client.send_raw_transaction(&tx).c(d!())
    }
}
