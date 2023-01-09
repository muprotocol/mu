use std::borrow::Cow;

use musdk_common::{status::Status, Request, Response};
use serde::{Deserialize, Serialize};

use crate::{FromRequest, IntoResponse};

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
    type Error = Status;

    fn from_request(req: &'a Request) -> Result<Self, Self::Error> {
        serde_json::from_slice::<T>(&req.body)
            .map(Self)
            .map_err(|_| {
                //TODO: Log error back to runtime
                Status::BadRequest
            })
    }
}

impl<'a, T: Serialize> IntoResponse<'a> for Json<T> {
    fn into_response(self) -> Response<'a> {
        match serde_json::to_vec(&self.0) {
            Ok(vec) => Response::build()
                .content_type(Cow::Borrowed("application/json"))
                .body_from_vec(vec),

            //TODO: log the error back to runtime
            Err(_) => Status::InternalServerError.into_response(),
        }
    }
}
