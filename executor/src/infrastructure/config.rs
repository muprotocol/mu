use std::{str::FromStr, time::Duration};

use anyhow::{Context, Result};
use config::{Config, Environment, File, FileFormat};
use log::ParseLevelError;
use serde::Deserialize;

use crate::{
    gateway::GatewayManagerConfig,
    network::{
        connection_manager::ConnectionManagerConfig,
        gossip::{GossipConfig, KnownNodeConfig},
    },
};

use super::log_setup::{LogConfig, LogFilterConfig};

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

    let gossip_config = config
        .get::<GossipConfigRaw>("gossip")
        .context("Invalid gossip config")?
        .into();

    let known_node_config: Vec<KnownNodeConfig> = config
        .get("gossip.seeds")
        .context("Invalid known_node config")?;

    let gateway_config = config
        .get("gateway_manager")
        .context("Invalid gateway config")?;

    let log_config = config
        .get::<LogConfigRaw>("log")
        .context("Invalid log config")?
        .try_into()?;

    Ok((
        connection_manager_config,
        gossip_config,
        known_node_config,
        gateway_config,
        log_config,
    ))
}

// We can't directly deserialize `Duration` type, so instead make config in two steps
#[derive(Clone, Deserialize)]
struct GossipConfigRaw {
    pub heartbeat_interval_millis: u64,
    pub liveness_check_interval_millis: u64,
    pub assume_dead_after_missed_heartbeats: u32,
    pub max_peers: usize,
    pub peer_update_interval_millis: u64,
}

impl From<GossipConfigRaw> for GossipConfig {
    fn from(raw: GossipConfigRaw) -> Self {
        GossipConfig {
            heartbeat_interval: Duration::from_millis(raw.heartbeat_interval_millis),
            liveness_check_interval: Duration::from_millis(raw.liveness_check_interval_millis),
            assume_dead_after_missed_heartbeats: raw.assume_dead_after_missed_heartbeats,
            max_peers: raw.max_peers,
            peer_update_interval: Duration::from_millis(raw.peer_update_interval_millis),
        }
    }
}

// We can't directly deserialize `LevelFilter` type, so instead make config in two steps
#[derive(Deserialize)]
struct LogConfigRaw {
    level: String,
    filters: Vec<LogFilterConfigRaw>,
}

#[derive(Deserialize)]
struct LogFilterConfigRaw {
    module: String,
    level: String,
}

impl TryFrom<LogConfigRaw> for LogConfig {
    type Error = ParseLevelError;

    fn try_from(raw: LogConfigRaw) -> Result<Self, Self::Error> {
        let level = log::LevelFilter::from_str(&raw.level)?;
        Ok(LogConfig {
            level,
            filters: raw
                .filters
                .into_iter()
                .map(TryInto::try_into)
                .collect::<Result<Vec<LogFilterConfig>, Self::Error>>()?,
        })
    }
}

impl TryFrom<LogFilterConfigRaw> for LogFilterConfig {
    type Error = ParseLevelError;
    fn try_from(raw: LogFilterConfigRaw) -> Result<Self, Self::Error> {
        let level = log::LevelFilter::from_str(&raw.level)?;
        Ok(LogFilterConfig {
            module: raw.module,
            level,
        })
    }
}
