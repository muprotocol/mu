mod serde_support;

pub use serde_support::{ConfigDuration, ConfigLogLevelFilter};

use anyhow::{Context, Result};
use config::{Config, Environment, File, FileFormat};

use crate::{
    gateway::GatewayManagerConfig,
    network::{
        connection_manager::ConnectionManagerConfig,
        gossip::{GossipConfig, KnownNodeConfig},
    },
};

use super::log_setup::LogConfig;

pub fn initialize_config() -> Result<(
    ConnectionManagerConfig,
    GossipConfig,
    Vec<KnownNodeConfig>,
    GatewayManagerConfig,
    LogConfig,
)> {
    let defaults = vec![
        ("log.level", "warn"),
        ("connection_manager.listen_ip", "0.0.0.0"),
        ("connection_manager.listen_port", "12012"),
        ("connection_manager.max_request_size_kb", "8192"),
        ("gossip.heartbeat_interval_millis", "1000"),
        ("gossip.assume_dead_after_missed_heartbeats", "10"),
        ("gossip.max_peers", "6"),
        ("gossip.peer_update_interval_millis", "10000"),
        ("gossip.liveness_check_interval_millis", "1000"),
        ("gateway_manager.listen_ip", "0.0.0.0"),
        ("gateway_manager.listen_port", "12012"),
    ];

    let default_arrays = vec!["log.filters", "gossip.seeds"];

    let env = Environment::default()
        .prefix("MU")
        .prefix_separator("__")
        .keep_prefix(false)
        .separator("__")
        .try_parsing(true);

    let mut builder = Config::builder();

    for (key, val) in defaults {
        builder = builder
            .set_default(key, val)
            .context("Failed to add default config")?;
    }

    for key in default_arrays {
        builder = builder
            .set_default(key, Vec::<String>::new())
            .context("Failed to add default array config")?;
    }

    builder = builder.add_source(File::new("mu-conf.yaml", FileFormat::Yaml));

    #[cfg(debug_assertions)]
    {
        if std::path::Path::new("mu-conf.dev.yaml").exists() {
            builder = builder.add_source(File::new("mu-conf.dev.yaml", FileFormat::Yaml));
        }
    }

    builder = builder.add_source(env);

    let config = builder
        .build()
        .context("Failed to initialize configuration")?;

    let connection_manager_config = config
        .get("connection_manager")
        .context("Invalid connection_manager config")?;

    let gossip_config = config.get("gossip").context("Invalid gossip config")?;

    let known_node_config: Vec<KnownNodeConfig> = config
        .get("gossip.seeds")
        .context("Invalid known_node config")?;

    let gateway_config = config
        .get("gateway_manager")
        .context("Invalid gateway config")?;

    let log_config = config
        .get::<LogConfig>("log")
        .context("Invalid log config")?;

    println!("###\nConfigs: {:?}", gossip_config);
    Ok((
        connection_manager_config,
        gossip_config,
        known_node_config,
        gateway_config,
        log_config,
    ))
}
