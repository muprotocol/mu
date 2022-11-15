use crate::runtime::error::Error;

use super::{FromMessage, Message};
use anyhow::Result;
use serde::Deserialize;

#[derive(Debug)]
pub struct Log {
    //TODO: timestamp: DateTime<Utc>,
    log: LogDetails,
}

#[derive(Debug, Deserialize)]
pub struct LogDetails {
    pub body: String,
}

impl FromMessage for Log {
    const TYPE: &'static str = "Log";

    fn from_message(m: Message) -> Result<Self, Error> {
        Ok(Self {
            log: serde_json::from_value(m.message)
                .map_err(|e| Error::MessageDeserializationFailed(e))?,
            //TODO: timestamp: chrono::Utc::now(),
        })
    }
}
