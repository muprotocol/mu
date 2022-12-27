use std::{
    borrow::Cow,
    io::{Read, Write},
};

use borsh::{BorshDeserialize, BorshSerialize};
use num_derive::FromPrimitive;
use num_traits::FromPrimitive;

use crate::{database::DatabaseRequest, Response};

#[repr(u16)]
#[derive(FromPrimitive)]
pub enum OutgoingMessageKind {
    FatalError = 1,
    FunctionResult = 2,
    Log = 3,
    DatabaseRequest = 4,
}

#[derive(BorshDeserialize, BorshSerialize)]
pub struct FatalError<'a> {
    pub error: Cow<'a, str>,
}

#[derive(Debug, BorshDeserialize, BorshSerialize)]
pub struct FunctionResult<'a> {
    pub response: Response<'a>,
}

#[derive(Debug, BorshDeserialize, BorshSerialize)]
pub struct Log<'a> {
    pub body: Cow<'a, str>,
    pub level: LogLevel,
}

#[repr(u8)]
#[derive(Debug, FromPrimitive, BorshDeserialize, BorshSerialize)]
pub enum LogLevel {
    Error = 0,
    Warn = 1,
    Info = 2,
    Debug = 3,
    Trace = 4,
}

pub enum OutgoingMessage<'a> {
    FatalError(FatalError<'a>),
    FunctionResult(FunctionResult<'a>),
    Log(Log<'a>),
    DatabaseRequest(DatabaseRequest<'a>),
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

        read_cases!(
            kind,
            reader,
            [FatalError, FunctionResult, Log, DatabaseRequest]
        )
    }

    pub fn write(&self, writer: &mut impl Write) -> std::io::Result<()> {
        write_cases!(
            self,
            writer,
            [FatalError, FunctionResult, Log, DatabaseRequest]
        );

        Ok(())
    }
}
