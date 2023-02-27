use std::{
    fs,
    net::{IpAddr, Ipv4Addr},
    path::{Path, PathBuf},
};

use anyhow::Result;

use mu_common::serde_support::IpOrHostname;
use mu_db::{DbConfig, DbManager, PdConfig, TcpPortAddress, TikvConfig, TikvRunnerConfig};

pub const DATA_SUBDIR: &str = ".mu/key_value_table";

pub async fn start(project_root: PathBuf) -> Result<Box<dyn DbManager>> {
    fn local_addr(port: u16) -> TcpPortAddress {
        TcpPortAddress {
            address: IpOrHostname::Ip(IpAddr::V4(Ipv4Addr::LOCALHOST)),
            port,
        }
    }

    fn subdir(dir: &Path, subdir: &'static str) -> Result<PathBuf> {
        let res = dir.join(subdir);
        fs::create_dir_all(&res)?;
        Ok(res)
    }

    let data_dir = project_root.join(DATA_SUBDIR);

    let node_address = local_addr(12012);

    let tikv_config = TikvRunnerConfig {
        pd: PdConfig {
            peer_url: local_addr(12385),
            client_url: local_addr(12386),
            data_dir: subdir(&data_dir, "pd_data")?,
            log_file: None,
        },
        node: TikvConfig {
            cluster_url: local_addr(20163),
            data_dir: subdir(&data_dir, "tikv_data")?,
            log_file: None,
        },
    };

    mu_db::start(
        node_address,
        vec![],
        DbConfig {
            external: None,
            internal: Some(tikv_config),
        },
    )
    .await
}
