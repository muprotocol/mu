use std::{
    collections::HashSet,
    net::{IpAddr, Ipv4Addr, Ipv6Addr},
};

use anyhow::{bail, Context};
use protobuf::{EnumOrUnknown, MessageField};

use crate::network::NodeAddress;

include!(concat!(env!("OUT_DIR"), "/protos/membership/mod.rs"));

impl From<(IpAddr, u16)> for membership::NodeAddress {
    fn from(a: (IpAddr, u16)) -> Self {
        let (address, port) = a;
        membership::NodeAddress {
            ip: MessageField(Some(Box::new(match address {
                IpAddr::V4(v4) => membership::IPAddress {
                    address: Some(membership::ipaddress::Address::Ipv4(v4.octets().into())),
                    ..Default::default()
                },
                IpAddr::V6(v6) => membership::IPAddress {
                    address: Some(membership::ipaddress::Address::Ipv6(v6.octets().into())),
                    ..Default::default()
                },
            }))),
            port: port as u32,
            ..Default::default()
        }
    }
}

impl TryFrom<membership::NodeAddress> for (IpAddr, u16) {
    type Error = anyhow::Error;

    fn try_from(a: membership::NodeAddress) -> Result<Self, Self::Error> {
        Ok((
            match a.ip.0.context("Received empty IP address")?.address {
                None => anyhow::bail!("Received empty IP address"),

                Some(membership::ipaddress::Address::Ipv4(bytes)) => {
                    let bytes: [u8; 4] = bytes
                        .try_into()
                        .map_err(|_| anyhow::anyhow!("Expected 4 bytes in an IPv4"))?;
                    IpAddr::V4(Ipv4Addr::from(bytes))
                }

                Some(membership::ipaddress::Address::Ipv6(bytes)) => {
                    let bytes: [u8; 16] = bytes
                        .try_into()
                        .map_err(|_| anyhow::anyhow!("Expected 16 bytes in an IPv6"))?;
                    IpAddr::V6(Ipv6Addr::from(bytes))
                }
            },
            a.port.try_into().context("Port was not a u16")?,
        ))
    }
}

impl From<super::NodeStatus> for membership::NodeStatus {
    fn from(n: super::NodeStatus) -> Self {
        fn convert_generation(g: u128) -> membership::Generation {
            // unwrap safety: we're making two 64 bit numbers via shift and bitwise and, so this should never fail.
            let upper = (g >> u64::BITS)
                .try_into()
                .context("Failed to separate generation into upper and lower half")
                .unwrap();
            let lower = (g & u64::MAX as u128)
                .try_into()
                .context("Failed to separate generation into upper and lower half")
                .unwrap();
            membership::Generation {
                lower,
                upper,
                ..Default::default()
            }
        }

        fn convert_stack_id(id: mu_stack::StackID) -> membership::StackID {
            match id {
                mu_stack::StackID::PWRStackID(k) => membership::StackID {
                    id: Some(membership::stack_id::Id::Solana(k.into())),
                    ..Default::default()
                },
            }
        }

        fn convert_timestamp(
            t: chrono::NaiveDateTime,
        ) -> protobuf::well_known_types::timestamp::Timestamp {
            let utc: chrono::NaiveDateTime = chrono::NaiveDate::from_ymd_opt(1970, 1, 1)
                .unwrap()
                .and_hms_opt(0, 0, 0)
                .unwrap();
            let duration = t
                .signed_duration_since(utc)
                .to_std()
                .expect("Expected current date/time to be after Unix epoch");
            protobuf::well_known_types::timestamp::Timestamp {
                seconds: duration.as_secs().try_into().unwrap(),
                nanos: duration.subsec_nanos().try_into().unwrap(),
                ..Default::default()
            }
        }

        fn convert_state(s: super::NodeState) -> EnumOrUnknown<membership::NodeState> {
            match s {
                super::NodeState::Dead => EnumOrUnknown::new(membership::NodeState::DEAD),
                super::NodeState::Alive => EnumOrUnknown::new(membership::NodeState::ALIVE),
            }
        }

        Self {
            version: n.version,
            generation: MessageField::some(convert_generation(n.address.generation)),
            region_id: n.region_id,
            last_update: MessageField::some(convert_timestamp(n.last_update)),
            state: convert_state(n.state),
            deployed_stacks: n
                .deployed_stacks
                .into_iter()
                .map(convert_stack_id)
                .collect(),
            ..Default::default()
        }
    }
}

impl TryFrom<(membership::NodeAddress, membership::NodeStatus)> for super::NodeStatus {
    type Error = anyhow::Error;

    fn try_from(m: (membership::NodeAddress, membership::NodeStatus)) -> Result<Self, Self::Error> {
        fn convert_generation(a: membership::Generation) -> u128 {
            ((a.upper as u128) << u64::BITS) | (a.lower as u128)
        }

        fn convert_stack_id(id: membership::StackID) -> anyhow::Result<mu_stack::StackID> {
            Ok(match id.id {
                None => anyhow::bail!("Received empty stack ID"),

                Some(membership::stack_id::Id::Solana(k)) => mu_stack::StackID::PWRStackID(
                    k.try_into()
                        .map_err(|_| anyhow::anyhow!("Expected 32 bytes for a Solana stack ID"))?,
                ),
            })
        }

        fn convert_timestamp(
            t: protobuf::well_known_types::timestamp::Timestamp,
        ) -> anyhow::Result<chrono::NaiveDateTime> {
            let epoch: chrono::NaiveDateTime = chrono::NaiveDate::from_ymd_opt(1970, 1, 1)
                .unwrap()
                .and_hms_opt(0, 0, 0)
                .unwrap();

            let duration = chrono::Duration::seconds(t.seconds)
                .checked_add(&chrono::Duration::nanoseconds(t.nanos.into()))
                .context("Failed to add nanos")?;

            epoch
                .checked_add_signed(duration)
                .context("Failed to add timestamp to epoch")
        }

        fn convert_state(
            s: EnumOrUnknown<membership::NodeState>,
        ) -> anyhow::Result<super::NodeState> {
            match s.enum_value() {
                Ok(membership::NodeState::ALIVE) => Ok(super::NodeState::Alive),
                Ok(membership::NodeState::DEAD) => Ok(super::NodeState::Dead),
                _ => bail!("Unknown node state value"),
            }
        }

        let (address, status) = m;

        let (ip, port) = address.try_into()?;

        Ok(Self {
            version: status.version,
            address: NodeAddress {
                address: ip,
                port,
                generation: convert_generation(
                    *status.generation.0.context("Got empty generation")?,
                ),
            },
            region_id: status.region_id,
            last_update: convert_timestamp(
                *status
                    .last_update
                    .0
                    .context("Got empty last update timestamp")?,
            )?,
            state: convert_state(status.state)?,
            deployed_stacks: status
                .deployed_stacks
                .into_iter()
                .map(convert_stack_id)
                .collect::<anyhow::Result<HashSet<_>>>()?,
        })
    }
}
