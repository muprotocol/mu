use serde::{Deserialize, Serialize};
use std::path::Path;

use super::{types::DatabaseID, Error};

pub type Config = ConfigBase<DatabaseID>;
pub type ConfigInner = ConfigBase<String>;

impl From<Config> for ConfigInner {
    fn from(conf: Config) -> Self {
        Self {
            database_id: conf.database_id.to_string(),
            cache_capacity: conf.cache_capacity,
            flush_every_ms: conf.flush_every_ms,
            segment_size: conf.segment_size,
            mode: conf.mode,
            use_compression: conf.use_compression,
            compression_factor: conf.compression_factor,
            print_profile_on_drop: conf.print_profile_on_drop,
            idgen_persist_interval: conf.idgen_persist_interval,
            temporary: conf.temporary,
        }
    }
}

impl TryFrom<ConfigInner> for Config {
    type Error = super::Error;
    fn try_from(conf: ConfigInner) -> Result<Self, Self::Error> {
        Ok(Self {
            database_id: conf.database_id.parse()?,
            cache_capacity: conf.cache_capacity,
            flush_every_ms: conf.flush_every_ms,
            segment_size: conf.segment_size,
            mode: conf.mode,
            use_compression: conf.use_compression,
            compression_factor: conf.compression_factor,
            print_profile_on_drop: conf.print_profile_on_drop,
            idgen_persist_interval: conf.idgen_persist_interval,
            temporary: conf.temporary,
        })
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub enum Mode {
    #[default]
    LowSpace,
    HighThroughput,
}

impl From<Mode> for sled::Mode {
    fn from(mode: Mode) -> Self {
        match mode {
            Mode::LowSpace => Self::LowSpace,
            Mode::HighThroughput => Self::HighThroughput,
        }
    }
}

/// # Path
///
/// `./mudb/`
///
/// # Default Database
///
/// `./mudb/default.mudb`
///
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ConfigBase<T>
where
    T: Default + ToString,
{
    pub database_id: T,
    // common
    pub cache_capacity: u64,
    pub flush_every_ms: Option<u64>,
    pub segment_size: usize,
    pub mode: Mode,
    pub use_compression: bool,
    pub compression_factor: i32,
    pub print_profile_on_drop: bool,
    pub idgen_persist_interval: u64,
    // useful in testing
    pub temporary: bool,
}

impl<T> Default for ConfigBase<T>
where
    T: Default + ToString,
{
    fn default() -> Self {
        Self {
            database_id: T::default(),
            cache_capacity: 1024 * 1024 * 1024, // 1gb
            flush_every_ms: Some(500),
            segment_size: 512 * 1024, // 512kb in bytes
            mode: Mode::LowSpace,
            use_compression: false,
            compression_factor: 5,
            print_profile_on_drop: false,
            idgen_persist_interval: 1_000_000,
            temporary: false,
        }
    }
}

impl<T> From<ConfigBase<T>> for sled::Config
where
    T: Default + ToString,
{
    fn from(cb: ConfigBase<T>) -> Self {
        let path = format! {"./mudb/{}", cb.database_id.to_string()};
        let path = Path::new(&path);
        Self::default()
            .path(path)
            .cache_capacity(cb.cache_capacity)
            .flush_every_ms(cb.flush_every_ms)
            .segment_size(cb.segment_size)
            .mode(cb.mode.into())
            .use_compression(cb.use_compression)
            .compression_factor(cb.compression_factor)
            .print_profile_on_drop(cb.print_profile_on_drop)
            .idgen_persist_interval(cb.idgen_persist_interval)
            .temporary(cb.temporary)
    }
}

impl<T> From<ConfigBase<T>> for super::types::Value
where
    T: Default + ToString + Serialize,
{
    fn from(cb: ConfigBase<T>) -> Self {
        Self::from(serde_json::to_value(cb).unwrap())
    }
}

impl<T> TryFrom<super::types::Value> for ConfigBase<T>
where
    T: Default + ToString + for<'a> Deserialize<'a>,
{
    type Error = Error;
    fn try_from(v: super::types::Value) -> Result<Self, Self::Error> {
        serde_json::from_value(v.into()).map_err(Into::into)
    }
}
