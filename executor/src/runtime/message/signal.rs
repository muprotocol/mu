use super::{Message, ToMessage};
use anyhow::{Context, Result};
use serde::Serialize;

#[derive(Debug)]
pub struct Signal {
    kind: SignalKind,
}

#[derive(Serialize, Debug)]
pub enum SignalKind {
    SIGTERM,
}

impl ToMessage for Signal {
    const TYPE: &'static str = "Signal";

    fn to_message(&self) -> Result<Message> {
        Ok(Message {
            id: None,
            r#type: Self::TYPE.to_owned(),
            message: serde_json::to_value(&self.kind)
                .context("signal request serialization failed")?,
        })
    }
}

impl Signal {
    pub fn term() -> Self {
        Signal {
            kind: SignalKind::SIGTERM,
        }
    }
}
