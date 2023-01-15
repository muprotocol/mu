use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

use anyhow::Context;
use protobuf::MessageField;

include!(concat!(env!("OUT_DIR"), "/protos/gossip/mod.rs"));

impl From<super::GossipProtocolMessage> for gossip::GossipMessage {
    fn from(m: super::GossipProtocolMessage) -> Self {
        fn convert_node_address(a: super::super::NodeAddress) -> gossip::NodeAddress {
            // unwrap safety: we're making two 64 bit numbers via shift and bitwise and, so this should never fail.
            let upper_generation = (a.generation >> u64::BITS)
                .try_into()
                .context("Failed to separate generation into upper and lower half")
                .unwrap();
            let lower_generation = (a.generation & u64::MAX as u128)
                .try_into()
                .context("Failed to separate generation into upper and lower half")
                .unwrap();
            gossip::NodeAddress {
                ip: MessageField(Some(Box::new(match a.address {
                    IpAddr::V4(v4) => gossip::IPAddress {
                        address: Some(gossip::ipaddress::Address::Ipv4(v4.octets().into())),
                        ..Default::default()
                    },
                    IpAddr::V6(v6) => gossip::IPAddress {
                        address: Some(gossip::ipaddress::Address::Ipv6(v6.octets().into())),
                        ..Default::default()
                    },
                }))),
                port: a.port as u32,
                lower_generation,
                upper_generation,
                ..Default::default()
            }
        }

        fn convert_stack_id(id: mu_stack::StackID) -> gossip::StackID {
            match id {
                mu_stack::StackID::SolanaPublicKey(k) => gossip::StackID {
                    id: Some(gossip::stack_id::Id::Solana(k.into())),
                    ..Default::default()
                },
            }
        }

        match m {
            super::GossipProtocolMessage::Goodbye(addr) => Self {
                message: Some(gossip::gossip_message::Message::Goodbye(
                    convert_node_address(addr),
                )),
                ..Default::default()
            },
            super::GossipProtocolMessage::Heartbeat(hb) => Self {
                message: Some(gossip::gossip_message::Message::Heartbeat(
                    gossip::Heartbeat {
                        distance: hb.distance,
                        node_address: MessageField(Some(Box::new(convert_node_address(
                            hb.node_address,
                        )))),
                        seq: hb.seq,
                        region_id: hb.region_id,
                        deployed_stacks: hb
                            .deployed_stacks
                            .into_iter()
                            .map(convert_stack_id)
                            .collect(),
                        ..Default::default()
                    },
                )),
                ..Default::default()
            },
        }
    }
}

impl TryFrom<gossip::GossipMessage> for super::GossipProtocolMessage {
    type Error = anyhow::Error;

    fn try_from(m: gossip::GossipMessage) -> Result<Self, Self::Error> {
        fn convert_node_address(
            a: gossip::NodeAddress,
        ) -> anyhow::Result<super::super::NodeAddress> {
            let generation =
                ((a.upper_generation as u128) << u64::BITS) | (a.lower_generation as u128);

            Ok(super::super::NodeAddress {
                port: a.port.try_into().context("Port was not a u16")?,
                generation,
                address: match a.ip.0.context("Received empty IP address")?.address {
                    None => anyhow::bail!("Received empty IP address"),

                    Some(gossip::ipaddress::Address::Ipv4(bytes)) => {
                        let bytes: [u8; 4] = bytes
                            .try_into()
                            .map_err(|_| anyhow::anyhow!("Expected 4 bytes in an IPv4"))?;
                        IpAddr::V4(Ipv4Addr::from(bytes))
                    }

                    Some(gossip::ipaddress::Address::Ipv6(bytes)) => {
                        let bytes: [u8; 16] = bytes
                            .try_into()
                            .map_err(|_| anyhow::anyhow!("Expected 16 bytes in an IPv6"))?;
                        IpAddr::V6(Ipv6Addr::from(bytes))
                    }
                },
            })
        }

        fn convert_stack_id(id: gossip::StackID) -> anyhow::Result<mu_stack::StackID> {
            Ok(match id.id {
                None => anyhow::bail!("Received empty stack ID"),

                Some(gossip::stack_id::Id::Solana(k)) => mu_stack::StackID::SolanaPublicKey(
                    k.try_into()
                        .map_err(|_| anyhow::anyhow!("Expected 32 bytes for a Solana stack ID"))?,
                ),
            })
        }

        match m.message {
            None => anyhow::bail!("Empty gossip message"),

            Some(gossip::gossip_message::Message::Goodbye(addr)) => Ok(
                super::GossipProtocolMessage::Goodbye(convert_node_address(addr)?),
            ),

            Some(gossip::gossip_message::Message::Heartbeat(hb)) => {
                Ok(super::GossipProtocolMessage::Heartbeat(super::Heartbeat {
                    seq: hb.seq,
                    distance: hb.distance,
                    node_address: convert_node_address(
                        *hb.node_address
                            .0
                            .context("Missing node_address in gossip message")?,
                    )?,
                    region_id: hb.region_id,
                    deployed_stacks: hb
                        .deployed_stacks
                        .into_iter()
                        .map(convert_stack_id)
                        .collect::<anyhow::Result<Vec<_>>>()?,
                }))
            }
        }
    }
}
