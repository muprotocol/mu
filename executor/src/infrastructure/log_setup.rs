use std::collections::HashMap;

use config::{Config, Value};

use anyhow::{Context, Ok, Result};
use env_logger::Builder;
use log::LevelFilter;

use crate::config::ConfigExt;

pub fn setup(config: &Config) -> Result<()> {
    let table = config.get_table("log")?;

    let mut builder = Builder::new();

    builder.filter_level(parse_level(&table, "level", "log")?);

    let module_filters = crate::config::array_or_map_value(
        table
            .get("filters")
            .cloned()
            .unwrap_or_else(|| Value::new(None, Vec::<String>::new())),
        "log.filters",
    )?;

    for (idx, val) in module_filters.enumerate() {
        let table = val
            .into_table()
            .context(format!("Expected log.filters[{idx}] to be a table"))?;

        let path = format!("log.filters[{idx}]");

        let module = table
            .get_mandatory("module", path.as_str())?
            .clone()
            .into_string()
            .context(format!("Expected log.filters[{idx}].module to be a string"))?;
        let level = parse_level(&table, "level", path.as_str())?;

        builder.filter(Some(module.as_str()), level);
    }

    builder.init();

    Ok(())
}

fn parse_level(table: &HashMap<String, Value>, key: &str, path: &str) -> Result<LevelFilter> {
    Ok(table
        .get_mandatory(key, path)?
        .clone()
        .into_string()?
        .parse::<LevelFilter>()?)
}
