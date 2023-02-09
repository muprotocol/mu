pub mod db;

use std::{
    borrow::Cow,
    io::{Read, Write},
};

use borsh::{BorshDeserialize, BorshSerialize};
use num_derive::FromPrimitive;
use num_traits::FromPrimitive;

use crate::Request;
use db::*;

#[repr(u16)]
#[derive(FromPrimitive)]
enum IncomingMessageKind {
    // Runtime messages
    ExecuteFunction = 1,

    // DB Messages
    DbError = 1001,
    SingleResult = 1002,
    ListResult = 1003,
    KvPairsResult = 1004,
    TkPairsResult = 1005,
    TkvTriplesResult = 1006,
    EmptyResult = 1007,
    CasResult = 1008,
}

#[derive(Debug, BorshDeserialize, BorshSerialize)]
pub struct ExecuteFunction<'a> {
    pub function: Cow<'a, str>,
    pub request: Request<'a>,
}

#[derive(Debug)]
pub enum IncomingMessage<'a> {
    // Runtime messages
    ExecuteFunction(ExecuteFunction<'a>),

    // DB messages
    DbError(DbError<'a>),
    SingleResult(SingleResult<'a>),
    ListResult(ListResult<'a>),
    KvPairsResult(KvPairsResult<'a>),
    TkPairsResult(TkPairsResult<'a>),
    TkvTriplesResult(TkvTriplesResult<'a>),
    EmptyResult(EmptyResult),
    CasResult(CasResult<'a>),
}

macro_rules! read_cases {
    ($kind: ident, $reader: ident, [$($case: ident),+] * $lf: lifetime, [$($unit_case: ident),*]) => {
        match IncomingMessageKind::from_u16($kind) {
            $(Some(IncomingMessageKind::$case) => {
                let message: $case<$lf> = BorshDeserialize::deserialize_reader($reader)?;
                Ok(Self::$case(message))
            })+

            $(Some(IncomingMessageKind::$unit_case) => {
                let message: $unit_case = BorshDeserialize::deserialize_reader($reader)?;
                Ok(Self::$unit_case(message))
            })*

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

        read_cases!(
            kind,
            reader,
            [
                ExecuteFunction,
                DbError,
                SingleResult,
                ListResult,
                KvPairsResult,
                TkPairsResult,
                TkvTriplesResult,
                CasResult
            ] * 'static,
            [EmptyResult]
        )
    }

    pub fn write(&self, writer: &mut impl Write) -> std::io::Result<()> {
        write_cases!(
            self,
            writer,
            [
                ExecuteFunction,
                DbError,
                SingleResult,
                ListResult,
                KvPairsResult,
                TkPairsResult,
                TkvTriplesResult,
                EmptyResult,
                CasResult
            ]
        );

        Ok(())
    }
}
