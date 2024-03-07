use anyhow::Result;
use ethers::{
    providers::{Http, Middleware, Provider},
    types::{
        transaction::{eip2718::TypedTransaction, optimism::DepositTransaction},
        TransactionRequest, H160, H256, U256,
    },
};

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
    ) -> Result<TypedTransaction> {
        let mut tx = TransactionRequest::new().value(value).from(from);
        log::info!("eth from address: {:?}", from);

        if let Some(to) = to {
            tx = tx.to(to);
        }

        let nonce = self.provider.get_transaction_count(from, None).await?;
        let tx = tx.nonce(nonce).data(data.to_vec());

        let mut tx = TypedTransaction::DepositTransaction(DepositTransaction {
            tx,
            source_hash: H256::default(),
            mint: if value.is_zero() { None } else { Some(value) },
            is_system_tx: false,
        });

        self.provider.fill_transaction(&mut tx, None).await?;
        Ok(tx)
    }
}
