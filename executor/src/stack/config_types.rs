use base58::FromBase58;
use serde::{
    de::{Unexpected, Visitor},
    Deserialize,
};
use solana_sdk::{pubkey::Pubkey, signature::Keypair};

pub struct Base58PublicKey {
    pub public_key: Pubkey,
}

impl<'de> Deserialize<'de> for Base58PublicKey {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        deserializer.deserialize_str(Base58PublicKeyVisitor())
    }
}

struct Base58PublicKeyVisitor();

impl<'de> Visitor<'de> for Base58PublicKeyVisitor {
    type Value = Base58PublicKey;

    fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(formatter, "A base58-encoded public key")
    }

    fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        v.parse::<Pubkey>()
            .map(|public_key| Base58PublicKey { public_key })
            .map_err(|_| E::invalid_value(Unexpected::Str(v), &self))
    }
}

pub struct Base58PrivateKey {
    pub keypair: Keypair,
}

impl<'de> Deserialize<'de> for Base58PrivateKey {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        deserializer.deserialize_str(Base58PrivateKeyVisitor())
    }
}

struct Base58PrivateKeyVisitor();

impl<'de> Visitor<'de> for Base58PrivateKeyVisitor {
    type Value = Base58PrivateKey;

    fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(formatter, "A base58-encoded private key")
    }

    fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        v.from_base58()
            .map_err(|_| E::invalid_value(Unexpected::Str(v), &self))
            .and_then(|bytes| {
                Keypair::from_bytes(bytes.as_ref())
                    .map_err(|_| E::invalid_value(Unexpected::Str(v), &self))
            })
            .map(|keypair| Base58PrivateKey { keypair })
    }
}
