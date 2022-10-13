mod serde_support;

pub use serde_support::{ConfigDuration, ConfigLogLevelFilter};

use anyhow::{Context, Result};
use config::{Config, Environment, File, FileFormat};

use crate::{
    gateway::GatewayManagerConfig,
    log_setup::LogConfig,
    network::{
        connection_manager::ConnectionManagerConfig,
        gossip::{GossipConfig, KnownNodeConfig},
    },
    runtime::types::RuntimeConfig,
    stack::{blockchain_monitor::BlockchainMonitorConfig, scheduler::SchedulerConfig},
};

pub struct SystemConfig(
    pub ConnectionManagerConfig,
    pub GossipConfig,
    pub Vec<KnownNodeConfig>,
    pub GatewayManagerConfig,
    pub LogConfig,
    pub RuntimeConfig,
    pub SchedulerConfig,
    pub BlockchainMonitorConfig,
);

pub fn initialize_config() -> Result<SystemConfig> {
    let defaults = vec![
        ("log.level", "warn"),
        ("connection_manager.listen_ip", "0.0.0.0"),
        ("connection_manager.listen_port", "12012"),
        ("connection_manager.max_request_size_kb", "8192"),
        ("gossip.heartbeat_interval", "1s"),
        ("gossip.assume_dead_after_missed_heartbeats", "10"),
        ("gossip.max_peers", "6"),
        ("gossip.peer_update_interval", "10s"),
        ("gossip.liveness_check_interval", "1s"),
        ("gateway_manager.listen_ip", "0.0.0.0"),
        ("gateway_manager.listen_port", "12012"),
        ("scheduler.tick_interval", "1s"),
        ("blockchain_monitor.solana_cluster_rpc_url", "https://api.mainnet-beta.solana.com:8899/"),
        ("blockchain_monitor.solana_cluster_pub_sub_url", "https://api.mainnet-beta.solana.com:8900/"),
        ("blockchain_monitor.solana_provider_public_key", "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA"),
        ("blockchain_monitor.solana_region_number", "1"),
        ("blockchain_monitor.solana_usage_signer_private_key", "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA"),
        ("blockchain_monitor.solana_min_escrow_balance", "50"),
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

    let log_config = config.get("log").context("Invalid log config")?;

    let runtime_config = config.get("runtime").context("Invalid runtime config")?;

    let scheduler_config = config
        .get("scheduler")
        .context("Invalid scheduler config")?;

    let blockchain_monitor_config = config
        .get("blockchain_monitor")
        .context("Invalid blockchain mnonitor config")?;

    Ok(SystemConfig(
        connection_manager_config,
        gossip_config,
        known_node_config,
        gateway_config,
        log_config,
        runtime_config,
        scheduler_config,
        blockchain_monitor_config,
    ))
}
