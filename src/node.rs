use std::{fs, mem::size_of, path::PathBuf, sync::Arc, thread::sleep, time::Duration};

use anyhow::{anyhow, Result};
use bitcoincore_rpc::{Auth, Client};
use clap::Args;
use config::Config;
use da::DAServiceManager;
use fetcher::{Data, Fetcher};
use json_rpc_server::serve;
use rpc_server::handle::NovoHandle;
use rt_evm::{
    model::{traits::BlockStorage, types::UnsignedTransaction},
    EvmRuntime,
};

#[derive(Debug, Args)]
pub struct Node {
    #[clap(short, long)]
    config: String,

    #[clap(short, long, default_value_t = 1)]
    start: u64,

    #[clap(short, long)]
    datadir: String,

    #[clap(short, long)]
    listen_ip: String,

    #[clap(short, long)]
    api_port: u16,

    #[clap(short, long)]
    http_port: u16,

    #[clap(short, long)]
    ws_port: u16,
}

const FETCHER_HEIGHT_FILE: &str = "FETCHER_RUNTIME_height.meta";
const FETCHER_CONFIG_FILE: &str = "FETCHER_RUNTIME_chain_cfg.meta";

impl Node {
    pub async fn exeute(&self) -> Result<()> {
        let cfg = Config::new(&self.config)?;

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
                #[cfg(feature = "ethereum")]
                cfg.ethereum,
            )
            .await?,
        );

        let client = Arc::new(Client::new(
            &cfg.btc.btc_url,
            Auth::UserPass(cfg.btc.username.clone(), cfg.btc.password.clone()),
        )?);

        let datadir = PathBuf::from(&self.datadir);

        if !datadir.exists() {
            let start = if self.start > 0 { self.start } else { 1 };

            if let Err(e) = self
                .init_data_dir(
                    client.clone(),
                    da_mgr.clone(),
                    &cfg.btc.electrs_url,
                    start,
                    0,
                )
                .await
            {
                log::error!("init_data_dir error:{}", e);
                let _ = fs::remove_dir_all(datadir);

                return Err(e);
            }
        }

        let mut evm_rt = EvmRuntime::restore()
            .map_err(|e| anyhow!(e.to_string()))?
            .ok_or(anyhow!("restore data error"))?;

        self.start_eth_api_server(&evm_rt).await?;
        self.start_api_server(
            da_mgr.clone(),
            client.clone(),
            cfg.btc.da_fee,
            &cfg.btc.fee_address,
            &cfg.btc.network,
        )?;

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
            client,
            da_mgr,
            &cfg.btc.electrs_url,
            start,
            evm_rt.chain_id as u32,
        )
        .await?;
        log::info!("start node");

        loop {
            let (block_time, datas) = if let Ok(Some(block)) = fetcher.fetcher().await {
                block
            } else {
                sleep(Duration::from_secs(1));
                continue;
            };
            let hdr = evm_rt
                .generate_blockproducer(Default::default(), block_time)
                .map_err(|e| anyhow!(e.to_string()))?;
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
                    Data::Transaction(mut tx) => {
                        if let UnsignedTransaction::Deposit(ref mut tx) = tx.transaction.unsigned {
                            tx.nonce =
                                hdr.get_nonce(tx.from, None).map_err(|e| anyhow!("{}", e))?;
                        }
                        txs.push(*tx)
                    }
                }
            }
            log::debug!("execute transaction:{:#?}", txs);
            log::info!("execute transaction:{}", txs.len());

            hdr.produce_block(txs).map_err(|e| anyhow!(e.to_string()))?;
        }
    }

    async fn init_data_dir(
        &self,
        client: Arc<Client>,
        da_mgr: Arc<DAServiceManager>,
        electrs_url: &str,
        start: u64,
        chain_id: u32,
    ) -> Result<()> {
        log::info!("fetcher first config");
        let (height, cfg) = Fetcher::new(client, da_mgr, electrs_url, start, chain_id)
            .await?
            .fetcher_first_cfg()
            .await?;

        log::info!("create data dir");
        vsdb::vsdb_set_base_dir(&self.datadir).map_err(|e| anyhow!(e.to_string()))?;
        let datadir = vsdb::vsdb_get_base_dir();

        log::info!("init data dir");
        EvmRuntime::restore_or_create(cfg.chain_id.into(), &[])
            .map_err(|e| anyhow!(e.to_string()))?;
        fs::write(datadir.join(FETCHER_HEIGHT_FILE), u64::to_be_bytes(height))?;
        fs::write(
            datadir.join(FETCHER_CONFIG_FILE),
            serde_json::to_string_pretty(&cfg)?,
        )?;
        Ok(())
    }

    async fn start_eth_api_server(&self, evm_rt: &EvmRuntime) -> Result<()> {
        let http_endpoint = if 0 == self.http_port {
            None
        } else {
            Some(format!("{}:{}", self.listen_ip, self.http_port))
        };

        let ws_endpoint = if 0 == self.ws_port {
            None
        } else {
            Some(format!("{}:{}", self.listen_ip, self.ws_port))
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
        &self,
        da_mgr: Arc<DAServiceManager>,
        client: Arc<Client>,
        da_fee: u64,
        fee_address: &str,
        network: &str,
    ) -> Result<()> {
        let handle = NovoHandle::new(
            da_mgr.clone(),
            client.to_owned(),
            da_fee,
            fee_address,
            network,
        )?;
        let addr = format!("{}:{}", self.listen_ip, self.api_port).parse()?;

        tokio::spawn(async move {
            if let Err(e) = serve(&addr, handle).await {
                log::error!("api server execute error:{}", e);
            }
        });
        Ok(())
    }
}
