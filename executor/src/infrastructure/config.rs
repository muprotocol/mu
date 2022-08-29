use std::{collections::HashMap, time::Duration};

use anyhow::{Context, Result};
use config::{Config, Environment, File, FileFormat, Value};

use crate::{
    gateway::GatewayManagerConfig,
    network::{connection_manager::ConnectionManagerConfig, gossip::GossipConfig},
};

pub fn initialize_config() -> Result<(
    Config,
    ConnectionManagerConfig,
    GossipConfig,
    GatewayManagerConfig,
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

    let default_arrays = vec!["log.filters"];

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

    let connection_manager_config = ConnectionManagerConfig {
        listen_address: config
            .get_string("connection_manager.listen_address")?
            .parse()
            .context("Failed to parse listen_address")?,
        listen_port: config
            .get_string("connection_manager.listen_port")?
            .parse()
            .context("Failed to parse listen_port")?,
        max_request_response_size: config
            .get_string("connection_manager.max_request_size_kb")?
            .parse::<usize>()
            .context("Failed to parse max_request_response_size")?
            * 1024,
    };

    let gossip_config = GossipConfig {
        heartbeat_interval: Duration::from_millis(
            config
                .get_string("gossip.heartbeat_interval_millis")?
                .parse()
                .context("Failed to parse heartbeat_interval")?,
        ),
        assume_dead_after_missed_heartbeats: config
            .get_string("gossip.assume_dead_after_missed_heartbeats")?
            .parse()
            .context("Failed to parse assume_dead_after_missed_heartbeats")?,
        max_peers: config
            .get_string("gossip.max_peers")?
            .parse()
            .context("Failed to parse max_peers")?,
        peer_update_interval: Duration::from_millis(
            config
                .get_string("gossip.peer_update_interval_millis")?
                .parse()
                .context("Failed to parse peer_update_interval_millis")?,
        ),
        liveness_check_interval: Duration::from_millis(
            config
                .get_string("gossip.liveness_check_interval_millis")?
                .parse()
                .context("Failed to parse liveness_check_interval_millis")?,
        ),
    };

    let gateway_config = GatewayManagerConfig {
        listen_address: config
            .get_string("gateway_manager.listen_address")?
            .parse()
            .context("Failed to parse gateway_manager.listen_address")?,
        listen_port: config
            .get_string("gateway_manager.listen_port")?
            .parse()
            .context("Failed to parse gateway_manager.listen_port")?,
    };

    Ok((
        config,
        connection_manager_config,
        gossip_config,
        gateway_config,
    ))
}

pub trait ConfigExt {
    fn get_mandatory(&self, key: &str, path: &str) -> Result<&Value>;
}

impl ConfigExt for HashMap<String, Value> {
    fn get_mandatory(&self, key: &str, path: &str) -> Result<&Value> {
        self.get(key)
            .context(format!("Missing mandatory config value {path}.{key}"))
    }
}
