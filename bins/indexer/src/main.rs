#![deny(warnings)]

use std::{net::SocketAddr, sync::Arc};

use anyhow::{anyhow, Result};
use bitcoincore_rpc::{Auth, Client};
use clap::Parser;
use config::Config;
use da::DAServiceManager;
use fetcher::Fetcher;
use json_rpc_server::serve;
use rpc_server::handle::ApiHandle;
use rt_evm::{model::traits::BlockStorage, EvmRuntime};
use tokio::time::{sleep, Duration};

#[derive(Debug, Parser)]
pub struct CommandLine {
    #[clap(long)]
    pub config: String,
    #[clap(long)]
    pub datadir: String,
    #[clap(long)]
    pub listen_ip: String,
    #[clap(long)]
    pub api_port: u16,
    #[clap(long)]
    pub http_port: u16,
    #[clap(long)]
    pub ws_port: u16,
}

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::init();
    let cmd = CommandLine::parse();
    let cfg = Config::new(&cmd.config)?;

    vsdb::vsdb_set_base_dir(&cmd.datadir).map_err(|e| anyhow!(e.to_string()))?;

    let da_mgr = Arc::new(
        DAServiceManager::new(
            cfg.default_da,
            cfg.file,
            cfg.ipfs,
            cfg.celestia,
            cfg.greenfield,
        )
        .await?,
    );

    let evm_rt = Arc::new(
        EvmRuntime::restore_or_create(cfg.node.chain_id as u64, &[])
            .map_err(|e| anyhow!(e.to_string()))?,
    );

    let start = evm_rt
        .copy_storage_handler()
        .get_latest_block_header()
        .map_err(|e| anyhow!(e.to_string()))?
        .number;

    let http_endpoint = if 0 == cmd.http_port {
        None
    } else {
        Some(format!("{}:{}", cmd.listen_ip, cmd.http_port))
    };

    let ws_endpoint = if 0 == cmd.ws_port {
        None
    } else {
        Some(format!("{}:{}", cmd.listen_ip, cmd.ws_port))
    };

    evm_rt
        .spawn_jsonrpc_server(
            "novolite-0.1.0",
            http_endpoint.as_deref(),
            ws_endpoint.as_deref(),
        )
        .await
        .map_err(|e| anyhow!(e.to_string()))?;

    let client = Arc::new(Client::new(
        &cfg.btc.btc_url,
        Auth::UserPass(cfg.btc.username.clone(), cfg.btc.password.clone()),
    )?);

    let handle = ApiHandle::new(da_mgr.clone(), client.to_owned());
    let addr: SocketAddr = format!("{}:{}", cmd.listen_ip, cmd.api_port).parse()?;

    tokio::spawn(async move {
        if let Err(e) = serve(&addr, handle).await {
            log::error!("api server execute error:{}", e);
        }
    });

    let mut fetcher = Fetcher::new(
        &cfg.btc.electrs_url,
        client,
        start + 1,
        cfg.node.chain_id,
        da_mgr,
    )
    .await?;

    loop {
        let block = if let Ok(Some(block)) = fetcher.get_block().await {
            block
        } else {
            sleep(Duration::from_secs(1)).await;
            continue;
        };
        let mut txs = vec![];
        for btc_tx in block.txdata.iter() {
            match fetcher.decode_transaction(btc_tx).await {
                Ok(evm_txs) => {
                    for i in evm_txs.iter() {
                        if evm_rt.check_signed_tx(i).is_ok() {
                            txs.push(i.clone());
                        }
                    }
                }
                Err(e) => log::debug!("decode_transaction error:{}", e),
            }
        }
        log::debug!("execute transaction:{:#?}", txs);
        log::info!("execute transaction:{}", txs.len());
        let hdr = evm_rt
            .generate_blockproducer(Default::default(), block.header.time as u64)
            .map_err(|e| anyhow!(e.to_string()))?;
        hdr.produce_block(txs).map_err(|e| anyhow!(e.to_string()))?;
    }
}
