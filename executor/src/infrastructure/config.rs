use std::{str::FromStr, time::Duration};

use anyhow::{Context, Result};
use config::{Config, Environment, File, FileFormat};
use log::ParseLevelError;
use serde::{de::Visitor, Deserialize};

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

    let gossip_config = config.get("gossip").context("Invalid gossip config")?;

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

    println!("###\nConfigs: {:?}", gossip_config);
    Ok((
        connection_manager_config,
        gossip_config,
        known_node_config,
        gateway_config,
        log_config,
    ))
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

pub fn human_readable_duration_deserializer<'de, D>(d: D) -> Result<Duration, D::Error>
where
    D: serde::de::Deserializer<'de>,
{
    d.deserialize_str(HumanReadableDurationVisitor)
}

struct HumanReadableDurationVisitor;

impl<'de> Visitor<'de> for HumanReadableDurationVisitor {
    type Value = Duration;

    fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(
            formatter,
            "an unsigned integer (u64) followd by a unit(`s` for seconds, `m` for millis, `n` for nanos)"
        )
    }

    fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        if v.len() < 2 {
            return Err(E::custom("length must be at least 2"));
        }

        let (value, unit) = v.split_at(v.len() - 1);

        let value = value.parse::<u64>().map_err(|_| {
            E::invalid_value(serde::de::Unexpected::Str(value), &"unsigned integer")
        })?;

        let duration = match unit {
            "s" => Duration::from_secs(value),
            "m" => Duration::from_millis(value),
            "n" => Duration::from_nanos(value),
            u => {
                return Err(E::invalid_value(
                    serde::de::Unexpected::Str(u),
                    &"`s`, `m` or `n`",
                ))
            }
        };

        Ok(duration)
    }
}
