pub mod db;

use std::{
    borrow::Cow,
    io::{Read, Write},
};

use borsh::{BorshDeserialize, BorshSerialize};
use num_derive::FromPrimitive;
use num_traits::FromPrimitive;

use crate::Response;
use db::*;

#[repr(u16)]
#[derive(FromPrimitive)]
pub enum OutgoingMessageKind {
    // Runtime messages
    FatalError = 1,
    FunctionResult = 2,
    Log = 3,

    // DB messages
    Put = 1001,
    Get = 1002,
    Delete = 1003,
    DeleteByPrefix = 1004,
    Scan = 1005,
    TableList = 1006,
    BatchPut = 1007,
    BatchGet = 1008,
    BatchDelete = 1009,
    BatchScan = 1010,
    // TODO
    // ScanKeys = 1007,
    // BatchScanKeys = 1013,
    // CompareAndSwap = 1014,
}

#[derive(Debug, BorshDeserialize, BorshSerialize)]
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

#[derive(Debug)]
pub enum OutgoingMessage<'a> {
    // Runtime messages
    FatalError(FatalError<'a>),
    FunctionResult(FunctionResult<'a>),
    Log(Log<'a>),

    // DB messages
    Put(Put<'a>),
    Get(Get<'a>),
    Delete(Delete<'a>),
    DeleteByPrefix(DeleteByPrefix<'a>),
    Scan(Scan<'a>),
    TableList(TableList<'a>),
    BatchPut(BatchPut<'a>),
    BatchGet(BatchGet<'a>),
    BatchDelete(BatchDelete<'a>),
    BatchScan(BatchScan<'a>),
    // TODO
    // ScanKeys(ScanKeys<'a>),
    // BatchScanKeys(BatchScanKeys<'a>),
    // CompareAndSwap(CompareAndSwap<'a>),
}

macro_rules! read_cases {
    ($kind: ident, $reader: ident, [$($case: ident),+]) => {
        match OutgoingMessageKind::from_u16($kind) {
            $(Some(OutgoingMessageKind::$case) => {
                let message: $case<'static> = BorshDeserialize::deserialize_reader($reader)?;
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
            [
                FatalError,
                FunctionResult,
                Log,
                Put,
                Get,
                Delete,
                DeleteByPrefix,
                Scan,
                TableList,
                BatchPut,
                BatchGet,
                BatchDelete,
                BatchScan
            ]
        )
    }

    pub fn write(&self, writer: &mut impl Write) -> std::io::Result<()> {
        write_cases!(
            self,
            writer,
            [
                FatalError,
                FunctionResult,
                Log,
                Put,
                Get,
                Delete,
                DeleteByPrefix,
                Scan,
                TableList,
                BatchPut,
                BatchGet,
                BatchDelete,
                BatchScan
            ]
        );

        Ok(())
    }
}
