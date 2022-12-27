use std::borrow::Cow;

use borsh::BorshDeserialize;

#[derive(BorshDeserialize)]
pub struct Request<'a> {
    pub path: Cow<'a, str>,
    pub body: Cow<'a, [u8]>,
}

pub trait FromRequest<'a> {
    fn from_request(req: &'a Request) -> Self;
}

impl<'a> FromRequest<'a> for &'a Request<'a> {
    fn from_request(req: &'a Request) -> Self {
        req
    }
}

pub struct BinaryBody<'a> {
    pub body: &'a [u8],
}

impl<'a> FromRequest<'a> for BinaryBody<'a> {
    fn from_request(req: &'a Request) -> Self {
        Self { body: &req.body }
    }
}
