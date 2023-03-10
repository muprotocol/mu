pub mod db;
pub mod storage;

use std::{
    borrow::Cow,
    io::{Read, Write},
};

use borsh::{BorshDeserialize, BorshSerialize};
use num_derive::FromPrimitive;
use num_traits::FromPrimitive;

use crate::{function::*, http_client::Request as HttpRequest};
use db::*;
use storage::*;

#[derive(FromPrimitive)]
#[repr(u16)]
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
    ScanKeys = 1006,
    TableList = 1007,
    BatchPut = 1008,
    BatchGet = 1009,
    BatchDelete = 1010,
    BatchScan = 1011,
    BatchScanKeys = 1012,
    CompareAndSwap = 1013,

    // Storage messages
    StoragePut = 2001,
    StorageGet = 2002,
    StorageDelete = 2003,
    StorageList = 2004,

    // Http Client
    HttpRequest = 3001,
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
    ScanKeys(ScanKeys<'a>),
    TableList(TableList<'a>),
    BatchPut(BatchPut<'a>),
    BatchGet(BatchGet<'a>),
    BatchDelete(BatchDelete<'a>),
    BatchScan(BatchScan<'a>),
    BatchScanKeys(BatchScanKeys<'a>),
    CompareAndSwap(CompareAndSwap<'a>),

    // Storage messages
    StoragePut(StoragePut<'a>),
    StorageGet(StorageGet<'a>),
    StorageDelete(StorageDelete<'a>),
    StorageList(StorageList<'a>),

    // Http Client
    HttpRequest(HttpRequest<'a>),
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
                    format!("Unknown outgoing message code: {}", $kind)
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
                ScanKeys,
                TableList,
                BatchPut,
                BatchGet,
                BatchDelete,
                BatchScan,
                BatchScanKeys,
                CompareAndSwap,
                StoragePut,
                StorageGet,
                StorageDelete,
                StorageList,
                HttpRequest
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
                ScanKeys,
                TableList,
                BatchPut,
                BatchGet,
                BatchDelete,
                BatchScan,
                BatchScanKeys,
                CompareAndSwap,
                StoragePut,
                StorageGet,
                StorageDelete,
                StorageList,
                HttpRequest
            ]
        );

        Ok(())
    }
}
