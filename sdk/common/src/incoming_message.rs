use std::{
    borrow::Cow,
    io::{Read, Write},
};

use borsh::{BorshDeserialize, BorshSerialize};
use num_derive::FromPrimitive;
use num_traits::FromPrimitive;

use crate::{database::DatabaseResponse, request::Request};

#[repr(u16)]
#[derive(FromPrimitive)]
enum IncomingMessageKind {
    ExecuteFunction = 1,
    DatabaseResponse = 2,
}

#[derive(Debug, BorshDeserialize, BorshSerialize)]
pub struct ExecuteFunction<'a> {
    pub function: Cow<'a, str>,
    pub request: Request<'a>,
}

#[allow(dead_code)]
pub enum IncomingMessage<'a> {
    ExecuteFunction(ExecuteFunction<'a>),
    DatabaseResponse(DatabaseResponse<'a>),
}

macro_rules! read_cases {
    ($kind: ident, $reader: ident, [$($case: ident),+]) => {
        match IncomingMessageKind::from_u16($kind) {
            $(Some(IncomingMessageKind::$case) => {
                let message: $case<'static> = BorshDeserialize::try_from_reader($reader)?;
                Ok(Self::$case(message))
            })+

            None => Err(
                std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    format!("Unknown incoming message code: {}", $kind)
                )
            ),
        }
    };
}

macro_rules! write_cases {
    ($self: ident, $writer: ident, [$($case: ident),+]) => {
        match $self {
            $(IncomingMessage::$case(x) => {
                (IncomingMessageKind::$case as u16).serialize($writer)?;
                x.serialize($writer)?;
            })+
        }
    };
}

impl<'a> IncomingMessage<'a> {
    pub fn read(reader: &mut impl Read) -> std::io::Result<Self> {
        let kind: u16 = BorshDeserialize::deserialize_reader(reader)?;

        read_cases!(kind, reader, [ExecuteFunction, DatabaseResponse])
    }

    pub fn write(&self, writer: &mut impl Write) -> std::io::Result<()> {
        write_cases!(self, writer, [ExecuteFunction, DatabaseResponse]);

        Ok(())
    }
}
