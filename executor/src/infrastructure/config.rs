use std::{collections::HashMap, time::Duration};

use anyhow::{anyhow, Context, Result};
use config::{Config, Environment, File, FileFormat, Value};

use crate::{
    gateway::GatewayManagerConfig,
    network::{
        connection_manager::ConnectionManagerConfig,
        gossip::{GossipConfig, KnownNodeConfig},
    },
};

pub fn initialize_config() -> Result<(
    Config,
    ConnectionManagerConfig,
    GossipConfig,
    Vec<KnownNodeConfig>,
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

    let mut known_node_config = vec![];
    for (idx, val) in config
        .get_array("gossip.seeds")
        .context("Failed to get gossip.seeds as array")?
        .into_iter()
        .enumerate()
    {
        let table = val
            .into_table()
            .context(format!("Expected gossip.seeds[{idx}] to be an object"))?;
        known_node_config.push(KnownNodeConfig {
            address: table
                .get("address")
                .ok_or(anyhow!(
                    "Missing required key `address` in gossip.seeds[{idx}]"
                ))?
                .clone()
                .into_string()
                .context(format!(
                    "Expected gossip.seeds[{idx}].address to be a string"
                ))?
                .parse()
                .context(format!("Failed to parse gossip.seeds.address[{idx}]"))?,
            port: table
                .get("port")
                .ok_or(anyhow!(
                    "Missing required key `port` in gossip.seeds[{idx}]"
                ))?
                .clone()
                .into_string()
                .context(format!("Expected gossip.seeds[{idx}].port to be a number"))?
                .parse()
                .context(format!("Failed to parse gossip.seeds[{idx}].port"))?,
        });
    }

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
        known_node_config,
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
