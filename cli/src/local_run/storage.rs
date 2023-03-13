use std::net::{IpAddr, Ipv4Addr};

use anyhow::Result;
use mu_common::serde_support::{IpOrHostname, TcpPortAddress};
use mu_storage::{StorageConfig, StorageManager};
use storage_embedded_juicefs::{InternalStorageConfig, StorageInfo};

pub async fn start() -> Result<Box<dyn StorageManager>> {
    let addr = |port| TcpPortAddress {
        address: IpOrHostname::Ip(IpAddr::V4(Ipv4Addr::LOCALHOST)),
        port,
    };

    let tikv_endpoint = addr(12386);

    let config = StorageConfig {
        external: None,
        internal: Some(InternalStorageConfig {
            metadata_tikv_endpoints: vec![tikv_endpoint.clone()],
            object_storage_tikv_endpoints: vec![tikv_endpoint],
            storage: StorageInfo {
                endpoint: addr(3089),
            },
        }),
    };

    mu_storage::start(&config).await
}
