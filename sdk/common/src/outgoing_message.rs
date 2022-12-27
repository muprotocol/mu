use std::{borrow::Cow, io::Write};

use borsh::BorshSerialize;
use num_derive::FromPrimitive;

use crate::{
    error::{Error, Result},
    response::Response,
};

#[repr(u16)]
#[derive(FromPrimitive)]
pub enum OutgoingMessageKind {
    FatalError = 1,
    FunctionResult = 2,
}

#[derive(BorshSerialize)]
pub struct FatalError<'a> {
    pub error: Cow<'a, str>,
}

#[derive(BorshSerialize)]
pub struct FunctionResult<'a> {
    pub response: Response<'a>,
}

pub enum OutgoingMessage<'a> {
    FatalError(FatalError<'a>),
    FunctionResult(FunctionResult<'a>),
}

macro_rules! write_cases {
    ($self: ident, $writer: ident, [$($case: ident),+]) => {
        match $self {
            $(OutgoingMessage::$case(x) => {
                (OutgoingMessageKind::$case as u16).serialize($writer).map_err(Error::CannotSerializeOutgoingMessage)?;
                x.serialize($writer).map_err(Error::CannotSerializeOutgoingMessage)?;
            })+
        }
    };
}

impl<'a> OutgoingMessage<'a> {
    pub fn write(&self, writer: &mut impl Write) -> Result<()> {
        write_cases!(self, writer, [FatalError, FunctionResult]);

        Ok(())
    }
}
