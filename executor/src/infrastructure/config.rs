use std::collections::HashMap;

use anyhow::{Context, Result};
use config::{Config, Environment, File, FileFormat, Value};

pub fn initialize_config(defaults: Vec<(&str, &str)>) -> Result<Config> {
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

    builder = builder.add_source(File::new("mu-conf.yaml", FileFormat::Yaml));

    #[cfg(debug_assertions)]
    {
        if std::path::Path::new("mu-conf.dev.yaml").exists() {
            builder = builder.add_source(File::new("mu-conf.dev.yaml", FileFormat::Yaml));
        }
    }

    builder = builder.add_source(env);

    builder
        .build()
        .context("Failed to initialize configuration")
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
