use std::borrow::Cow;

use musdk_common::{
    incoming_message::IncomingMessage,
    outgoing_message::{db::Get, OutgoingMessage},
};

use crate::{Error, Result};

pub struct DbHandle<'a> {
    pub(super) context: &'a mut super::MuContext,
}

impl<'a> DbHandle<'a> {
    pub fn get<'b>(&mut self, key: impl Into<&'b [u8]>) -> Result<Vec<u8>> {
        let request = Get {
            key: Cow::Borrowed(key.into()),
        };
        self.context.write_message(OutgoingMessage::Get(request))?;
        let response = self.context.read_message()?;
        match response {
            IncomingMessage::DBError(s) => Err(Error::DatabaseError(s.error.into_owned())),
            IncomingMessage::SingleResult(s) => Ok(s.value.into_owned()),
            _ => Err(Error::UnexpectedMessageKind("Get")),
        }
    }

    // TODO: implement other request types
}
