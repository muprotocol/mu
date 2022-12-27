use std::borrow::Cow;

use borsh::{BorshDeserialize, BorshSerialize};

use crate::error::Result;

#[derive(BorshSerialize, BorshDeserialize)]
pub struct Response<'a> {
    body: Cow<'a, [u8]>,
}

pub trait IntoResponse<'a> {
    fn into_response(self) -> Response<'a>;
}

pub trait TryIntoResponse<'a> {
    fn try_into_response(self) -> Result<Response<'a>>;
}

impl<'a> IntoResponse<'a> for Response<'a> {
    fn into_response(self) -> Response<'a> {
        self
    }
}

impl<'a, T> TryIntoResponse<'a> for T
where
    T: IntoResponse<'a>,
{
    fn try_into_response(self) -> Result<Response<'a>> {
        Ok(self.into_response())
    }
}

// TODO: make generic over errors, http status codes?
impl<'a, T> TryIntoResponse<'a> for Result<T>
where
    T: IntoResponse<'a>,
{
    fn try_into_response(self) -> Result<Response<'a>> {
        self.map(IntoResponse::into_response)
    }
}

pub struct BinaryResponse {
    body: Vec<u8>,
}

impl BinaryResponse {
    pub fn new(body: Vec<u8>) -> Self {
        Self { body }
    }
}

impl<'a> IntoResponse<'a> for BinaryResponse {
    fn into_response(self) -> Response<'a> {
        Response {
            body: Cow::Owned(self.body),
        }
    }
}
