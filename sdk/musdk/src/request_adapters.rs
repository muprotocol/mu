use musdk_common::Request;

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
