use serde::Deserialize;
use std::path::Path;

/// # Defalut Config
///
/// ```ignore
/// path: Path::new("./mudb/default.mudb"),
/// // other from sled
/// ```
///
/// *Sled Default Config*
///
/// [link](https://docs.rs/sled/0.34.7/src/sled/config.rs.html#221)
///
/// ```ignore
/// // generally useful
/// create_new: false,
/// cache_capacity: 1024 * 1024 * 1024, // 1gb
/// mode: sled::Mode::LowSpace,
/// use_compression: false,
/// compression_factor: 5,
/// temporary: false,
/// // useful in testing
/// print_profile_on_drop: false,
/// flush_every_ms: Some(500),
/// idgen_persist_interval: 1_000_000,
/// ```
#[derive(Clone, Debug, Deserialize)]
pub struct Config {
    // TODO: split into per-database config and system-wide config
    pub name: String,
    pub cache_capacity: Option<u64>,
    pub flush_every_ms: Option<Option<u64>>,
    pub segment_size: Option<usize>,
    pub mode: Option<Mode>,
    pub use_compression: Option<bool>,
    pub compression_factor: Option<i32>,
    pub print_profile_on_drop: Option<bool>,
    pub idgen_persist_interval: Option<u64>,
    // useful in testing
    pub temporary: Option<bool>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            name: "default.mudb".to_string(),
            cache_capacity: None,
            flush_every_ms: None,
            segment_size: None,
            mode: None,
            use_compression: None,
            compression_factor: None,
            print_profile_on_drop: None,
            idgen_persist_interval: None,
            temporary: None,
        }
    }
}

#[derive(Clone, Debug, Deserialize)]
pub enum Mode {
    LowSpace,
    HighThroughput,
}

impl From<Config> for sled::Config {
    fn from(conf: Config) -> Self {
        let mut inner = sled::Config::default();
        if let Some(to) = conf.cache_capacity {
            inner = inner.cache_capacity(to);
        }

        if let Some(to) = conf.flush_every_ms {
            inner = inner.flush_every_ms(to);
        }

        if let Some(to) = conf.segment_size {
            inner = inner.segment_size(to);
        }

        inner = inner.path(Path::new(&format! {"./mudb/{}", conf.name}));

        if let Some(to) = conf.temporary {
            inner = inner.temporary(to);
        }

        if let Some(to) = conf.use_compression {
            inner = inner.use_compression(to);
        }

        if let Some(to) = conf.compression_factor {
            inner = inner.compression_factor(to);
        }

        if let Some(to) = conf.print_profile_on_drop {
            inner = inner.print_profile_on_drop(to);
        }

        if let Some(to) = conf.idgen_persist_interval {
            inner = inner.idgen_persist_interval(to);
        }

        inner = match conf.mode {
            Some(Mode::LowSpace) => inner.mode(sled::Mode::LowSpace),
            Some(Mode::HighThroughput) => inner.mode(sled::Mode::HighThroughput),
            None => inner,
        };

        inner
    }
}
