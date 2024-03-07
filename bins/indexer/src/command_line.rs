use std::{fs, mem::size_of, path::PathBuf, sync::Arc, thread::sleep, time::Duration};

use anyhow::{anyhow, Result};
use bitcoincore_rpc::{Auth, Client};
use clap::Parser;
use config::{BtcConfig, Config};
#[cfg(feature = "celestia")]
use da::CelestiaConfig;
#[cfg(feature = "file")]
use da::FileConfig;
#[cfg(feature = "greenfield")]
use da::GreenfieldConfig;
#[cfg(feature = "ipfs")]
use da::IpfsConfig;
use da::{DAServiceManager, DaType};
use fetcher::{Data, Fetcher};
use json_rpc_server::serve;
use rpc_server::handle::NovoHandle;
use rt_evm::{model::traits::BlockStorage, EvmRuntime};

#[derive(Debug, Parser)]
pub struct CommandLine {
    ///Generate example configuration file
    #[arg(short, long)]
    generate: bool,
    #[clap(long)]
    config: String,
    #[clap(long, default_value_t = 1)]
    start: u64,
    #[clap(long)]
    datadir: String,
    #[clap(long)]
    listen_ip: String,
    #[clap(long)]
    api_port: u16,
    #[clap(long)]
    http_port: u16,
    #[clap(long)]
    ws_port: u16,
}

const FETCHER_HEIGHT_FILE: &str = "FETCHER_RUNTIME_height.meta";
const FETCHER_CONFIG_FILE: &str = "FETCHER_RUNTIME_chain_cfg.meta";

impl CommandLine {
    pub async fn exeute(&self) -> Result<()> {
        if self.generate {
            return self.generate_config().await;
        }
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
            )
            .await?,
        );

        let client = Arc::new(Client::new(
            &cfg.btc.btc_url,
            Auth::UserPass(cfg.btc.username.clone(), cfg.btc.password.clone()),
        )?);

        let datadir = PathBuf::from(&self.datadir);

        if !datadir.exists() {
            if let Err(e) = self
                .init_data_dir(
                    &cfg.btc.electrs_url,
                    client.clone(),
                    da_mgr.clone(),
                    &cfg.btc.fee_address,
                    cfg.btc.da_fee,
                    &cfg.btc.network,
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

        self.start_eth_api_server(&evm_rt).await?;
        self.start_api_server(
            da_mgr.clone(),
            client.clone(),
            &cfg.btc.fee_address,
            cfg.btc.da_fee,
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
            &cfg.btc.electrs_url,
            client,
            start,
            evm_rt.chain_id as u32,
            da_mgr,
            &cfg.btc.fee_address,
            cfg.btc.da_fee,
            &cfg.btc.network,
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

    pub async fn generate_config(&self) -> Result<()> {
        let file = PathBuf::from(&self.config);
        if file.exists() {
            Err(anyhow!("Configuration file already exists"))
        } else {
            let cfg = Config {
                default_da: DaType::Greenfield,
                #[cfg(feature = "file")]
                file: Some(FileConfig {
                    path: "/path/to/data".to_string(),
                }),
                #[cfg(feature = "ipfs")]
                ipfs: Some(IpfsConfig {
                    url: "http://127.0.0.1:5001".to_string(),
                }),
                #[cfg(feature = "celestia")]
                celestia: Some(CelestiaConfig {
                    url: "http://127.0.0.1:8080".to_string(),
                    token: "vefbrebqrber".to_string(),
                    id: "12345".to_string(),
                }),
                #[cfg(feature = "greenfield")]
                greenfield: Some(GreenfieldConfig {
                    rpc_addr: "https://gnfd-testnet-fullnode-tendermint-us.bnbchain.org:443"
                        .to_string(),
                    chain_id: "greenfield_5600-1".to_string(),
                    bucket: "bucketname".to_string(),
                    password_file: "/tmp/password".to_string(),
                }),
                btc: BtcConfig {
                    electrs_url: "tcp://127.0.0.1:60401".to_string(),
                    btc_url: "http://127.0.0.1:18443".to_string(),
                    username: "admin1".to_string(),
                    password: "123".to_string(),
                    network: "regtest".to_string(),
                    da_fee: 100,
                    fee_address: "bcrt1qhwkqamxr93phyhlc82elqm2n8hufr8xls0djwn".to_string(),
                },
            };
            Ok(fs::write(file, serde_json::to_string_pretty(&cfg)?)?)
        }
    }
    async fn init_data_dir(
        &self,
        electrs_url: &str,
        client: Arc<Client>,
        da_mgr: Arc<DAServiceManager>,
        fee_address: &str,
        da_fee: u64,
        network: &str,
    ) -> Result<()> {
        log::info!("fetcher first config");
        let (height, cfg) = Fetcher::new(
            electrs_url,
            client,
            self.start,
            0,
            da_mgr.clone(),
            fee_address,
            da_fee,
            network,
        )
        .await?
        .fetcher_first_cfg()
        .await?;

        log::info!("create data dir");
        vsdb::vsdb_set_base_dir(&self.datadir).map_err(|e| anyhow!(e.to_string()))?;
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
        fee_address: &str,
        da_fee: u64,
    ) -> Result<()> {
        let handle = NovoHandle::new(da_mgr.clone(), client.to_owned(), da_fee, fee_address);
        let addr = format!("{}:{}", self.listen_ip, self.api_port).parse()?;

        tokio::spawn(async move {
            if let Err(e) = serve(&addr, handle).await {
                log::error!("api server execute error:{}", e);
            }
        });
        Ok(())
    }
}
