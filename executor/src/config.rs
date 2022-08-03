use anyhow::{Context, Result};
use config::{Config, Environment, File, FileFormat};

pub fn initialize_config() -> Result<Config> {
    let env = Environment::default()
        .prefix("MU")
        .prefix_separator("__")
        .keep_prefix(false)
        .separator("__")
        .try_parsing(true);

    let mut builder = Config::builder()
        .set_default("log_level", "warn")
        .unwrap()
        .add_source(File::new("mu-conf.yaml", FileFormat::Yaml));

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
