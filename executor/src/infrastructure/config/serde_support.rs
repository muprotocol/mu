use std::{ops::Deref, str::FromStr, time::Duration};

use serde::{
    de::{self, Visitor},
    Deserialize, Deserializer,
};

// Wrapper type to support human-readable duration deserialization with serde
#[derive(Debug, Clone)]
pub struct ConfigDuration(Duration);

impl ConfigDuration {
    pub fn new(d: Duration) -> Self {
        Self(d)
    }
}

impl Deref for ConfigDuration {
    type Target = Duration;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl From<Duration> for ConfigDuration {
    fn from(d: Duration) -> Self {
        Self(d)
    }
}

impl<'de> Deserialize<'de> for ConfigDuration {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer
            .deserialize_str(HumanReadableDurationVisitor)
            .map(ConfigDuration)
    }
}

struct HumanReadableDurationVisitor;

impl<'de> Visitor<'de> for HumanReadableDurationVisitor {
    type Value = Duration;

    fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(
            formatter,
            "an unsigned integer (u64) followed by a unit: `d` for days, `h` for hours, `m` for minutes, `s` for seconds, `ms` for milliseconds, `us` for microseconds, `ns` for nanoseconds"
        )
    }

    fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        let split_offset = v
            .chars()
            .take_while(|c| c.is_numeric())
            .map(|c| c.len_utf8())
            .sum::<usize>();
        if split_offset == 0 || split_offset >= v.len() {
            return Err(E::invalid_value(de::Unexpected::Str(v), &self));
        }
        let (value, unit) = v.split_at(split_offset);

        let value = value
            .parse::<u64>()
            .map_err(|_| E::invalid_value(de::Unexpected::Str(value), &"an unsigned integer"))?;

        let duration = match unit {
            "d" => Duration::from_secs(value * 60 * 60 * 24),
            "h" => Duration::from_secs(value * 60 * 60),
            "m" => Duration::from_secs(value * 60),
            "s" => Duration::from_secs(value),
            "ms" => Duration::from_millis(value),
            "us" => Duration::from_micros(value),
            "ns" => Duration::from_nanos(value),
            u => {
                return Err(E::invalid_value(
                    de::Unexpected::Str(u),
                    &"a unit: `d`, `h`, `m`, `s`, `ms`, `us` or `ns`",
                ))
            }
        };

        Ok(duration)
    }
}

#[derive(Clone, Debug)]
pub struct ConfigLogLevelFilter(log::LevelFilter);

impl Deref for ConfigLogLevelFilter {
    type Target = log::LevelFilter;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<'de> Deserialize<'de> for ConfigLogLevelFilter {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_str(ConfigLogLevelFilterDeserializeVisitor)
    }
}

struct ConfigLogLevelFilterDeserializeVisitor;

impl<'de> Visitor<'de> for ConfigLogLevelFilterDeserializeVisitor {
    type Value = ConfigLogLevelFilter;

    fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(
            formatter,
            "one of `off`, `error`, `warn`, `info`, `debug`, `trace`"
        )
    }

    fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        let level = log::LevelFilter::from_str(v)
            .map_err(|_| E::invalid_value(de::Unexpected::Str(v), &self))?;
        Ok(ConfigLogLevelFilter(level))
    }
}
