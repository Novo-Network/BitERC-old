use anyhow::{anyhow, Result};
use async_trait::async_trait;
use celestia_rpc::{BlobClient, Client};
use celestia_types::{
    blob::SubmitOptions,
    consts::HASH_SIZE,
    nmt::{Namespace, NS_ID_V0_SIZE},
    Blob, Commitment,
};

use crate::{service::DAService, DaType};

pub struct CelestiaService {
    client: Client,
    namespace: Namespace,
}

impl CelestiaService {
    pub async fn new(
        url: &str,
        auth_token: Option<&str>,
        namespace_id: [u8; NS_ID_V0_SIZE],
    ) -> Result<Self> {
        Ok(Self {
            client: Client::new(url, auth_token).await?,
            namespace: Namespace::const_v0(namespace_id),
        })
    }
}

#[async_trait]
impl DAService for CelestiaService {
    async fn set_full_tx(&self, tx: &[u8]) -> Result<Vec<u8>> {
        let opts = SubmitOptions::default();
        let blob = Blob::new(self.namespace, tx.to_vec())?;
        let mut hash = blob.commitment.0.to_vec();
        let height = self.client.blob_submit(&[blob], opts).await?;
        hash.extend_from_slice(&height.to_be_bytes());
        Ok(hash)
    }

    async fn get_tx(&self, hash: &[u8]) -> Result<Vec<u8>> {
        if hash.len() < 40 {
            return Err(anyhow!("length error"));
        }
        let mut commitment: [u8; HASH_SIZE] = [0; HASH_SIZE];
        commitment.copy_from_slice(&hash[..HASH_SIZE]);
        let mut bytes: [u8; 8] = [0; 8];
        bytes.copy_from_slice(&hash[32..40]);
        let height = u64::from_be_bytes(bytes);
        let blob = self
            .client
            .blob_get(height, self.namespace, Commitment(commitment))
            .await?;
        blob.validate()?;
        Ok(blob.data)
    }

    fn type_byte(&self) -> u8 {
        DaType::Celestia.type_byte()
    }
}
