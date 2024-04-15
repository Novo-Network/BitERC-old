use anyhow::{anyhow, Result};
use ethers::{
    providers::{Http, Middleware, Provider},
    types::{
        transaction::{
            eip2718::TypedTransaction, optimism::DepositTransaction as EtherDepositTransaction,
        },
        TransactionRequest, H160, H256, U256,
    },
};
use rt_evm::model::types::{DepositTransaction, SignedTransaction, TransactionAction};
pub struct EthTransactionBuilder {
    provider: Provider<Http>,
}

impl EthTransactionBuilder {
    pub fn new(url: &str) -> Result<Self> {
        Ok(Self {
            provider: Provider::<Http>::try_from(url)?,
        })
    }
    pub async fn chain_id(&self) -> Result<u32> {
        Ok(self.provider.get_chainid().await?.as_u64() as u32)
    }
    pub async fn build_transaction(
        &self,
        from: H160,
        value: U256,
        to: Option<H160>,
        data: &[u8],
    ) -> Result<SignedTransaction> {
        let source_hash = H256::random();
        let gas = {
            let mut tx = TransactionRequest::new().value(value).from(from);
            log::info!("eth from address: {:?}", from);

            if let Some(to) = to {
                tx = tx.to(to);
            }

            let nonce = self.provider.get_transaction_count(from, None).await?;
            let tx = tx.nonce(nonce).data(data.to_vec());

            let mut tx = TypedTransaction::DepositTransaction(EtherDepositTransaction {
                tx,
                source_hash: source_hash.clone(),
                mint: if value.is_zero() { None } else { Some(value) },
                is_system_tx: false,
            });

            self.provider.fill_transaction(&mut tx, None).await?;
            tx.gas().cloned().ok_or(anyhow!("gas get failes"))?
        };
        let deposit_tx = DepositTransaction {
            nonce: self.provider.get_transaction_count(from, None).await?,
            source_hash,
            from,
            action: match to {
                Some(v) => TransactionAction::Call(v),
                None => TransactionAction::Create,
            },
            mint: if value.is_zero() { None } else { Some(value) },
            value,
            gas_limit: gas,
            is_system_tx: false,
            data: data.to_vec(),
        };
        let chain_id = self.provider.get_chainid().await?.as_u64();

        Ok(SignedTransaction::from_deposit_tx(deposit_tx, chain_id))
    }
}
