use std::{
    fs,
    net::{IpAddr, Ipv4Addr},
    path::{Path, PathBuf},
};

use anyhow::Result;
use mu_db::{DbConfig, DbManager, IpAndPort, PdConfig, TikvConfig, TikvRunnerConfig};

pub const DATA_SUBDIR: &str = "target/mu-temp/database";

pub async fn start(project_root: PathBuf) -> Result<Box<dyn DbManager>> {
    fn local_addr(port: u16) -> IpAndPort {
        IpAndPort {
            address: IpAddr::V4(Ipv4Addr::LOCALHOST),
            port,
        }
    }

    fn subdir(dir: &Path, subdir: &'static str) -> Result<PathBuf> {
        let mut res = dir.to_path_buf();
        res.push(subdir);
        fs::create_dir_all(&res)?;
        Ok(res)
    }

    let mut data_dir = project_root;
    data_dir.push(DATA_SUBDIR);

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
