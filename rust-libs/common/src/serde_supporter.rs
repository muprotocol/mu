use std::{fmt, net::IpAddr};

use anyhow::bail;
use serde::{
    de::{self, Visitor},
    Deserialize, Deserializer,
};

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

impl From<IpOrHostname> for String {
    fn from(value: IpOrHostname) -> Self {
        match value {
            IpOrHostname::Ip(ip) => ip.to_string(),
            IpOrHostname::Hostname(hostname) => hostname,
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

impl TryFrom<&str> for IpOrHostname {
    type Error = anyhow::Error;

    fn try_from(value: &str) -> std::result::Result<Self, Self::Error> {
        match value.parse::<IpAddr>() {
            Ok(ip) => Ok(IpOrHostname::Ip(ip)),
            Err(_) => {
                if hostname_validator::is_valid(value) {
                    return Ok(IpOrHostname::Hostname(value.to_owned()));
                }
                bail!("string is not a valid hostname or IP address")
            }
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
        write!(formatter, "a string containing either a hostname or an ip")
    }

    fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        IpOrHostname::try_from(v).map_err(|_| {
            E::invalid_value(de::Unexpected::Str(v), &"Invalid value for IpOrHostname")
        })
    }
}
