use std::{
    net::{IpAddr, Ipv4Addr},
    path::PathBuf,
};

use beau_collector::BeauCollector;
use mu_db::{DbManager, IpAndPort, PdConfig, TikvConfig, TikvRunnerConfig};

pub struct TempDir(PathBuf);

impl TempDir {
    pub fn new() -> Self {
        TempDir(std::env::temp_dir().join(TempDir::rand_dir_name()))
    }

    pub fn get_rand_sub_dir(&self, prefix: Option<&str>) -> PathBuf {
        let name = format!("{}{}", prefix.unwrap_or(""), Self::rand_dir_name());
        self.0.join(name)
    }

    fn rand_dir_name() -> String {
        let rand: [u8; 5] = rand::random();
        rand.into_iter()
            .fold(String::new(), |a, i| format!("{a}{i}"))
    }

    fn clean(&mut self) -> std::io::Result<()> {
        std::fs::remove_dir_all(&self.0)
    }
}

impl Default for TempDir {
    fn default() -> Self {
        Self::new()
    }
}

pub struct Database {
    pub db_manager: Box<dyn DbManager>,
    data_dir: TempDir,
}

impl Database {
    pub async fn start() -> anyhow::Result<Self> {
        let data_dir = TempDir::new();

        let addr = |port| IpAndPort {
            address: IpAddr::V4(Ipv4Addr::LOCALHOST),
            port,
        };

        let node_address = addr(12803);

        let tikv_config = TikvRunnerConfig {
            pd: PdConfig {
                peer_url: addr(12385),
                client_url: addr(12386),
                data_dir: data_dir.get_rand_sub_dir(Some("pd_data_dir")),
                log_file: Some(data_dir.get_rand_sub_dir(Some("pd_log"))),
            },
            node: TikvConfig {
                cluster_url: addr(20163),
                data_dir: data_dir.get_rand_sub_dir(Some("tikv_data_dir")),
                log_file: Some(data_dir.get_rand_sub_dir(Some("tikv_log"))),
            },
        };

        Ok(Self {
            db_manager: mu_db::new_with_embedded_cluster(node_address, vec![], tikv_config).await?,
            data_dir,
        })
    }

    pub async fn stop(mut self) -> anyhow::Result<()> {
        [
            DbManager::stop_embedded_cluster(&*self.db_manager).await,
            self.data_dir.clean().map_err(Into::into),
        ]
        .into_iter()
        .bcollect()
    }
}
