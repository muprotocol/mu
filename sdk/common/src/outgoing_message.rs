use std::{
    borrow::Cow,
    io::{Read, Write},
};

use borsh::{BorshDeserialize, BorshSerialize};
use num_derive::FromPrimitive;
use num_traits::FromPrimitive;

use crate::response::Response;

#[repr(u16)]
#[derive(FromPrimitive)]
pub enum OutgoingMessageKind {
    FatalError = 1,
    FunctionResult = 2,
}

#[derive(BorshDeserialize, BorshSerialize)]
pub struct FatalError<'a> {
    pub error: Cow<'a, str>,
}

#[derive(Debug, BorshDeserialize, BorshSerialize)]
pub struct FunctionResult<'a> {
    pub response: Response<'a>,
}

pub enum OutgoingMessage<'a> {
    FatalError(FatalError<'a>),
    FunctionResult(FunctionResult<'a>),
}

macro_rules! read_cases {
    ($kind: ident, $reader: ident, [$($case: ident),+]) => {
        match OutgoingMessageKind::from_u16($kind) {
            $(Some(OutgoingMessageKind::$case) => {
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
            $(OutgoingMessage::$case(x) => {
                (OutgoingMessageKind::$case as u16).serialize($writer)?;
                x.serialize($writer)?;
            })+
        }
    };
}

impl<'a> OutgoingMessage<'a> {
    pub fn read(reader: &mut impl Read) -> std::io::Result<Self> {
        let kind: u16 = BorshDeserialize::deserialize_reader(reader)?;

        read_cases!(kind, reader, [FatalError, FunctionResult])
    }

    pub fn write(&self, writer: &mut impl Write) -> std::io::Result<()> {
        write_cases!(self, writer, [FatalError, FunctionResult]);

        Ok(())
    }
}
