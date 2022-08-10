use std::collections::HashMap;

use anyhow::{Context, Result};
use config::{Config, Environment, File, FileFormat, Value};

use crate::network::connection_manager::ConnectionManagerConfig;

pub fn initialize_config() -> Result<(Config, ConnectionManagerConfig)> {
    let defaults = vec![
        ("log.level", "warn"),
        ("connection_manager.listen_ip", "0.0.0.0"),
        ("connection_manager.listen_port", "12012"),
        ("connection_manager.max_request_size_kb", "8192"),
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
            .context("Failed to parse listen address")?,
        listen_port: config
            .get_string("connection_manager.listen_port")?
            .parse()
            .context("Failed to parse listen port")?,
        max_request_response_size: config
            .get_string("connection_manager.max_request_size_kb")?
            .parse::<usize>()
            .context("Failed to parse max request size")?
            * 1024,
    };

    Ok((config, connection_manager_config))
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
