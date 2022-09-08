use anyhow::{Ok, Result};
use env_logger::Builder;
use serde::Deserialize;

use super::config::ConfigLogLevelFilter;

pub fn setup(config: LogConfig) -> Result<()> {
    let mut builder = Builder::new();

    builder.filter_level(*config.level);

    for filter in config.filters {
        builder.filter(Some(&filter.module), *filter.level);
    }

    builder.init();

    Ok(())
}

#[derive(Deserialize)]
pub struct LogConfig {
    pub level: ConfigLogLevelFilter,
    pub filters: Vec<LogFilterConfig>,
}

#[derive(Deserialize)]
pub struct LogFilterConfig {
    pub module: String,
    pub level: ConfigLogLevelFilter,
}
