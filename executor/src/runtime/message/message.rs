use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::any::type_name;
use uuid::Uuid;

#[derive(Serialize)]
pub struct InputMessage {
    pub id: Uuid,
    pub r#type: &'static str,
    pub message: String,
}

impl InputMessage {
    pub fn new<T>(message: T) -> Result<Self>
    where
        T: Input,
    {
        Ok(Self {
            id: Uuid::new_v4(),
            r#type: <T as Input>::get_type(),
            message: serde_json::to_string(&message)?,
        })
    }

    pub fn new_with_id<T>(id: Uuid, message: T) -> Result<Self>
    where
        T: Input,
    {
        Ok(Self {
            id,
            r#type: <T as Input>::get_type(),
            message: serde_json::to_string(&message)?,
        })
    }
}

impl Input for InputMessage {}

#[derive(Deserialize)]
pub struct OutputMessage {
    pub id: Uuid,
    pub r#type: String,
    pub message: String,
}

impl OutputMessage {
    pub fn new(message: String, r#type: String) -> Self {
        Self {
            id: Uuid::new_v4(),
            r#type,
            message,
        }
    }

    pub fn new_with_id(id: Uuid, message: String, r#type: String) -> Self {
        Self {
            id,
            r#type,
            message,
        }
    }
}

pub trait Input
where
    Self: Serialize,
{
    fn get_type() -> &'static str {
        type_name::<Self>()
    }
}
