use std::{borrow::Cow, io::Read};

use borsh::BorshDeserialize;
use num_derive::FromPrimitive;
use num_traits::FromPrimitive;

use crate::{
    error::{Error, Result},
    request::Request,
};

#[repr(u16)]
#[derive(FromPrimitive)]
enum IncomingMessageKind {
    ExecuteFunction = 1,
}

#[derive(BorshDeserialize)]
pub struct ExecuteFunction<'a> {
    pub function: Cow<'a, str>,
    pub request: Request<'a>,
}

#[allow(dead_code)]
pub enum IncomingMessage<'a> {
    ExecuteFunction(ExecuteFunction<'a>),
    SomethingElsePlaceholder,
}

macro_rules! read_cases {
    ($kind: ident, $reader: ident, [$($case: ident),+]) => {
        match IncomingMessageKind::from_u16($kind) {
            $(Some(IncomingMessageKind::$case) => {
                let message: $case<'static> = BorshDeserialize::try_from_reader($reader)
                    .map_err(Error::CannotDeserializeIncomingMessage)?;
                Ok(Self::$case(message))
            })+

            None => Err(Error::UnknownIncomingMessageCode($kind)),
        }
    };
}

impl<'a> IncomingMessage<'a> {
    pub fn read(reader: &mut impl Read) -> Result<Self> {
        let kind: u16 = BorshDeserialize::deserialize_reader(reader)
            .map_err(Error::CannotDeserializeIncomingMessage)?;

        read_cases!(kind, reader, [ExecuteFunction])
    }
}
