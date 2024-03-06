#![deny(warnings)]

use std::{fs, mem::size_of, path::PathBuf, sync::Arc, thread::sleep, time::Duration};

use anyhow::{anyhow, Result};
use bitcoincore_rpc::{Auth, Client};
use clap::Parser;
use config::Config;
use da::DAServiceManager;
use fetcher::{Data, Fetcher};
use json_rpc_server::serve;
use rpc_server::handle::ApiHandle;
use rt_evm::{model::traits::BlockStorage, EvmRuntime};

#[derive(Debug, Parser)]
pub struct CommandLine {
    #[clap(long)]
    pub config: String,
    #[clap(long, default_value_t = 1)]
    pub start: u64,
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

const FETCHER_HEIGHT_FILE: &str = "FETCHER_RUNTIME_height.meta";
const FETCHER_CONFIG_FILE: &str = "FETCHER_RUNTIME_chain_cfg.meta";

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::init();
    let cmd = CommandLine::parse();
    let cfg = Config::new(&cmd.config)?;

    let da_mgr = Arc::new(
        DAServiceManager::new(
            cfg.default_da,
            #[cfg(feature = "file")]
            cfg.file,
            #[cfg(feature = "ipfs")]
            cfg.ipfs,
            #[cfg(feature = "celestia")]
            cfg.celestia,
            #[cfg(feature = "greenfield")]
            cfg.greenfield,
        )
        .await?,
    );

    let client = Arc::new(Client::new(
        &cfg.btc.btc_url,
        Auth::UserPass(cfg.btc.username.clone(), cfg.btc.password.clone()),
    )?);

    let datadir = PathBuf::from(&cmd.datadir);

    if !datadir.exists() {
        if let Err(e) = init_data_dir(
            &cfg.btc.electrs_url,
            client.clone(),
            cmd.start,
            da_mgr.clone(),
            &cmd.datadir,
        )
        .await
        {
            log::error!("init_data_dir error:{}", e);
            let _ = fs::remove_dir_all(datadir);

            return Err(e);
        }
    }

    vsdb::vsdb_set_base_dir(&datadir).map_err(|e| anyhow!(e.to_string()))?;

    let mut evm_rt = EvmRuntime::restore()
        .map_err(|e| anyhow!(e.to_string()))?
        .ok_or(anyhow!("restore data error"))?;

    start_eth_api_server(&evm_rt, &cmd.listen_ip, cmd.http_port, cmd.ws_port).await?;
    start_api_server(da_mgr.clone(), client.clone(), &cmd.listen_ip, cmd.api_port)?;

    let start = {
        let height = fs::read(datadir.join(FETCHER_HEIGHT_FILE))?;
        let height = <[u8; size_of::<u64>()]>::try_from(height)
            .map(u64::from_be_bytes)
            .map_err(|_| anyhow!("start height read error"))?;

        let block_number = evm_rt
            .copy_storage_handler()
            .get_latest_block_header()
            .map_err(|e| anyhow!(e.to_string()))?
            .number;
        height + block_number
    };

    let mut fetcher = Fetcher::new(
        &cfg.btc.electrs_url,
        client,
        start,
        evm_rt.chain_id as u32,
        da_mgr,
    )
    .await?;

    loop {
        let (block_time, datas) = if let Ok(Some(block)) = fetcher.fetcher().await {
            block
        } else {
            sleep(Duration::from_secs(1));
            continue;
        };
        let mut txs = vec![];
        for data in datas {
            match data {
                Data::Config(cfg) => {
                    fetcher.chain_id = cfg.chain_id;
                    evm_rt.chain_id = cfg.chain_id.into();
                    fs::write(
                        datadir.join(FETCHER_CONFIG_FILE),
                        serde_json::to_string_pretty(&cfg)?,
                    )?;
                }
                Data::Transaction(tx) => txs.push(tx),
            }
        }
        log::debug!("execute transaction:{:#?}", txs);
        log::info!("execute transaction:{}", txs.len());
        let hdr = evm_rt
            .generate_blockproducer(Default::default(), block_time)
            .map_err(|e| anyhow!(e.to_string()))?;
        hdr.produce_block(txs).map_err(|e| anyhow!(e.to_string()))?;
    }
}

async fn init_data_dir(
    electrs_url: &str,
    client: Arc<Client>,
    start: u64,
    da_mgr: Arc<DAServiceManager>,
    datadir: &str,
) -> Result<()> {
    log::info!("fetcher first config");
    let (height, cfg) = Fetcher::new(electrs_url, client, start, 0, da_mgr.clone())
        .await?
        .fetcher_first_cfg()
        .await?;

    log::info!("create data dir");
    vsdb::vsdb_set_base_dir(datadir).map_err(|e| anyhow!(e.to_string()))?;
    let datadir = vsdb::vsdb_get_base_dir();

    log::info!("init data dir");
    EvmRuntime::create(cfg.chain_id.into(), &[]).map_err(|e| anyhow!(e.to_string()))?;
    fs::write(datadir.join(FETCHER_HEIGHT_FILE), u64::to_be_bytes(height))?;
    fs::write(
        datadir.join(FETCHER_CONFIG_FILE),
        serde_json::to_string_pretty(&cfg)?,
    )?;
    Ok(())
}

async fn start_eth_api_server(
    evm_rt: &EvmRuntime,
    listen_ip: &str,
    http_port: u16,
    ws_port: u16,
) -> Result<()> {
    let http_endpoint = if 0 == http_port {
        None
    } else {
        Some(format!("{}:{}", listen_ip, http_port))
    };

    let ws_endpoint = if 0 == ws_port {
        None
    } else {
        Some(format!("{}:{}", listen_ip, ws_port))
    };

    evm_rt
        .spawn_jsonrpc_server(
            "novolite-0.1.0",
            http_endpoint.as_deref(),
            ws_endpoint.as_deref(),
        )
        .await
        .map_err(|e| anyhow!(e.to_string()))
}

fn start_api_server(
    da_mgr: Arc<DAServiceManager>,
    client: Arc<Client>,
    listen_ip: &str,
    api_port: u16,
) -> Result<()> {
    let handle = ApiHandle::new(da_mgr.clone(), client.to_owned());
    let addr = format!("{}:{}", listen_ip, api_port).parse()?;

    tokio::spawn(async move {
        if let Err(e) = serve(&addr, handle).await {
            log::error!("api server execute error:{}", e);
        }
    });
    Ok(())
}
