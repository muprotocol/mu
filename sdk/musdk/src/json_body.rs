use std::borrow::Cow;

use musdk_common::{Request, Response, Status};
use serde::{Deserialize, Serialize};

use crate::{content_type, FromRequest, IntoResponse};

const JSON_CONTENT_TYPE: &str = "application/json";
const UTF8_CHARSET: &str = "charset=utf-8";

#[repr(transparent)]
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Json<T>(pub T);

impl<T> Json<T> {
    /// Consumes wrapper and returns wrapped item
    #[inline(always)]
    pub fn into_inner(self) -> T {
        self.0
    }
}

impl<'a, T: Deserialize<'a>> FromRequest<'a> for Json<T> {
    type Error = (&'static str, Status);

    fn from_request(req: &'a Request) -> Result<Self, Self::Error> {
        match req
            .content_type()
            .as_ref()
            .and_then(|s| content_type::parse(s).0)
        {
            Some(mime) if mime.to_lowercase() == JSON_CONTENT_TYPE => {
                serde_json::from_slice::<T>(&req.body)
                    .map(Self)
                    .map_err(|_| {
                        //TODO: Log error back to runtime
                        ("invalid json", Status::BadRequest)
                    })
            }
            Some(_) => Err((
                "content-type should be application/json",
                Status::BadRequest,
            )),
            None => Err(("content-type is missing", Status::BadRequest)),
        }
    }
}

impl<'a, T: Serialize> IntoResponse<'a> for Json<T> {
    fn into_response(self) -> Response<'a> {
        match serde_json::to_vec(&self.0) {
            Ok(vec) => Response::builder()
                .content_type(Cow::Borrowed(JSON_CONTENT_TYPE))
                .body_from_vec(vec),

            //TODO: log the error back to runtime
            Err(_) => Status::InternalServerError.into_response(),
        }
    }
}
