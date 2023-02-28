use std::{
    fmt::{self, Display},
    net::IpAddr,
};

use anyhow::bail;
use serde::{
    de::{self, Visitor},
    Deserialize, Deserializer,
};
use std::{ops::Deref, str::FromStr, time::Duration};

use http::Uri;

#[derive(Debug, Clone)]
pub enum IpOrHostname {
    Ip(IpAddr),
    Hostname(String),
}

impl IpOrHostname {
    pub fn is_unspecified(&self) -> bool {
        match self {
            IpOrHostname::Ip(ip) => ip.is_unspecified(),
            IpOrHostname::Hostname(_) => false,
        }
    }
}

impl FromStr for IpOrHostname {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.parse::<IpAddr>() {
            Ok(ip) => Ok(IpOrHostname::Ip(ip)),
            Err(_) => {
                if hostname_validator::is_valid(s) {
                    return Ok(IpOrHostname::Hostname(s.to_owned()));
                }
                bail!("string is not a valid hostname or IP address")
            }
        }
    }
}

impl fmt::Display for IpOrHostname {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            IpOrHostname::Hostname(h) => h.fmt(f),
            IpOrHostname::Ip(ip) => ip.fmt(f),
        }
    }
}

impl<'de> Deserialize<'de> for IpOrHostname {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_str(HumanReadableIpOrHostnameVisitor)
    }
}

struct HumanReadableIpOrHostnameVisitor;

impl<'de> Visitor<'de> for HumanReadableIpOrHostnameVisitor {
    type Value = IpOrHostname;

    fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(formatter, "A string containing either a hostname or an IP")
    }

    fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        v.parse::<IpOrHostname>()
            .map_err(|_| E::invalid_value(de::Unexpected::Str(v), &self))
    }
}

#[derive(Deserialize, Clone)]
pub struct TcpPortAddress {
    pub address: IpOrHostname,
    pub port: u16,
}

impl Display for TcpPortAddress {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}:{}", self.address, self.port)
    }
}

impl FromStr for TcpPortAddress {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        let parts: Vec<&str> = s.split(':').collect();
        if parts.len() != 2 {
            bail!("Can't parse, expected string in this format: ip_addr:port");
        } else {
            Ok(TcpPortAddress {
                address: parts[0].parse()?,
                port: parts[1].parse()?,
            })
        }
    }
}

impl From<TcpPortAddress> for String {
    fn from(value: TcpPortAddress) -> Self {
        value.to_string()
    }
}

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

#[derive(Clone, Debug)]
pub struct ConfigUri(pub Uri);

impl<'de> Deserialize<'de> for ConfigUri {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_str(ConfigUriDeserializeVisitor)
    }
}

struct ConfigUriDeserializeVisitor;

impl<'de> Visitor<'de> for ConfigUriDeserializeVisitor {
    type Value = ConfigUri;

    fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(formatter, "A valid URI")
    }

    fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        Ok(ConfigUri(Uri::from_str(v).map_err(|_| {
            E::invalid_value(de::Unexpected::Str(v), &self)
        })?))
    }
}
