use anyhow::{Ok, Result};
use env_logger::Builder;
use log::LevelFilter;
use serde::Deserialize;

pub fn setup(config: LogConfig) -> Result<()> {
    let mut builder = Builder::new();

    builder.filter_level(config.level);

    for filter in config.filters {
        builder.filter(Some(&filter.module), filter.level);
    }

    builder.init();

    Ok(())
}

#[derive(Deserialize)]
pub struct LogConfig {
    pub level: LevelFilter,
    pub filters: Vec<LogFilterConfig>,
}

#[derive(Deserialize)]
pub struct LogFilterConfig {
    pub module: String,
    pub level: LevelFilter,
}
