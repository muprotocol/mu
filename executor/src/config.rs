use anyhow::{Context, Result};
use config::{Config, File, FileFormat};

pub fn initialize_config() -> Result<Config> {
    let mut builder = Config::builder()
        .set_default("log-level", "warn")
        .unwrap()
        .add_source(File::new("mu-conf.yaml", FileFormat::Yaml));

    #[cfg(debug_assertions)]
    {
        if std::path::Path::new("mu-conf.dev.yaml").exists() {
            builder = builder.add_source(File::new("mu-conf.dev.yaml", FileFormat::Yaml));
        }
    }

    builder
        .build()
        .context("Failed to initialize configuration")
}
