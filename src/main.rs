#![deny(warnings)]

mod config;
mod fetcher_service;
mod tx;
mod vout_code;

use std::{sync::Arc, time::Duration};

use clap::Parser;
use config::Config;
use da::create_da_mgr;
use fetcher_service::FetcherService;
use rt_evm::{
    model::{traits::BlockStorage, types::H160},
    EvmRuntime,
};
use ruc::*;
use tokio::time::sleep;

#[derive(Debug, Parser)]
pub struct Args {
    #[clap(long)]
    config: String,
    #[clap(long)]
    datadir: String,
    #[clap(long)]
    listen: String,
    #[clap(long)]
    http_port: u16,
    #[clap(long)]
    ws_port: u16,
}

impl Args {
    pub async fn execute(self) -> Result<()> {
        let cfg = Config::new(&self.config)?;
        let da_mgr = create_da_mgr(
            cfg.file,
            cfg.file_path.as_deref(),
            cfg.ipfs,
            cfg.ipfs_url.as_deref(),
            cfg.celestia,
            cfg.celestia_url.as_deref(),
            cfg.celestia_token.as_deref(),
            cfg.celestia_namespace_id.as_deref(),
            cfg.greenfield,
            cfg.greenfield_rpc_addr.as_deref(),
            cfg.greenfield_chain_id.as_deref(),
            cfg.greenfield_bucket.as_deref(),
            cfg.greenfield_password_file.as_deref(),
            &cfg.default,
        )
        .await
        .map_err(|e| eg!(e))?;

        vsdb::vsdb_set_base_dir(&self.datadir).c(d!())?;

        let http_endpoint = if 0 == self.http_port {
            None
        } else {
            Some(format!("{}:{}", self.listen, self.http_port))
        };

        let ws_endpoint = if 0 == self.ws_port {
            None
        } else {
            Some(format!("{}:{}", self.listen, self.ws_port))
        };
        let evm_rt = Arc::new(EvmRuntime::restore_or_create(cfg.chain_id as u64, &[])?);
        let start = evm_rt
            .copy_storage_handler()
            .get_latest_block_header()?
            .number;
        evm_rt
            .spawn_jsonrpc_server(
                "novolite-0.1.0",
                http_endpoint.as_deref(),
                ws_endpoint.as_deref(),
            )
            .await
            .c(d!())?;
        let mut fetcher = FetcherService::new(
            &cfg.btc_url,
            &cfg.username,
            &cfg.password,
            start + 1,
            cfg.chain_id,
            Arc::new(da_mgr),
        )
        .await?;
        loop {
            if let Ok(Some(block)) = fetcher.get_block().await {
                let mut txs = vec![];
                for btc_tx in block.txdata.iter() {
                    match fetcher.decode_transaction(btc_tx).await {
                        Ok(evm_txs) => {
                            if !evm_txs.is_empty() {
                                for i in evm_txs.iter() {
                                    if evm_rt.check_signed_tx(i).is_ok() {
                                        txs.push(i.clone());
                                    }
                                }
                            }
                        }
                        Err(e) => log::debug!("decode_transaction error:{}", e),
                    }
                }
                log::debug!("execute transaction:{:#?}", txs);
                log::info!("execute transaction:{}", txs.len());
                let hdr = evm_rt
                    .generate_blockproducer(H160::default(), block.header.time as u64)
                    .c(d!())?;
                hdr.produce_block(txs).c(d!())?;
            } else {
                sleep(Duration::from_secs(1)).await;
            }
        }
    }
}

#[tokio::main]
async fn main() {
    env_logger::init();

    let args = Args::parse();

    args.execute().await.unwrap()
}
