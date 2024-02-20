use std::process::Command;

use ethers::{
    providers::{Http, Middleware, Provider},
    signers::{LocalWallet, Signer},
    types::{Bytes, TransactionRequest, H160, U256},
    utils::hex,
};
use ruc::*;

use super::SAT2WEI;

#[allow(unused)]
pub struct EthTransactionBuilder {
    provider: Provider<Http>,
    wallet: LocalWallet,
}

#[allow(unused)]
impl EthTransactionBuilder {
    pub async fn new(url: &str, sk: &str) -> Result<Self> {
        let provider = Provider::<Http>::try_from(url).c(d!())?;
        let chain_id = provider.get_chainid().await.c(d!())?.as_u64();
        let wallet = hex::decode(sk.strip_prefix("0x").unwrap_or(sk))
            .c(d!())
            .and_then(|bytes| LocalWallet::from_bytes(&bytes).c(d!()))
            .map(|wallet| wallet.with_chain_id(chain_id))?;
        Ok(Self { provider, wallet })
    }
    pub fn chain_id(&self) -> u32 {
        self.wallet.chain_id() as u32
    }
    pub async fn build_transaction(
        &self,
        value: U256,
        to: Option<H160>,
        data: &[u8],
        sig: &str,
        args: Vec<String>,
    ) -> Result<(Bytes, u64)> {
        let mut tx = TransactionRequest::new()
            .value(value)
            .from(self.wallet.address());
        log::info!("eth from address: {}", self.wallet.address());
        if let Some(to) = to {
            tx = tx.to(to);
        }
        let data = if data.is_empty() {
            let mut cast = Command::new("cast");
            cast.arg("calldata");
            cast.arg(sig);
            for it in args {
                cast.arg(it);
            }
            let output = cast.output().c(d!())?;
            let calldata = String::from_utf8(output.stdout).c(d!())?;
            if !output.status.success() {
                return Err(eg!(calldata));
            }
            hex::decode(calldata.trim().strip_prefix("0x").unwrap_or(&calldata)).c(d!())?
        } else {
            data.to_vec()
        };
        let nonce = self
            .provider
            .get_transaction_count(self.wallet.address(), None)
            .await
            .c(d!())?;
        let mut tx = tx.nonce(nonce).data(data).into();
        self.provider
            .fill_transaction(&mut tx, None)
            .await
            .c(d!())?;
        let signature = self.wallet.sign_transaction(&tx).await.c(d!())?;
        Ok((
            tx.rlp_signed(&signature),
            tx.gas()
                .c(d!())?
                .checked_div(U256::from(SAT2WEI))
                .c(d!())?
                .as_u64(),
        ))
    }
}
