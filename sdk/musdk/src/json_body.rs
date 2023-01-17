use std::borrow::Cow;

use musdk_common::{Request, Response, Status};
use serde::{Deserialize, Serialize};

use crate::{content_type, FromRequest, IntoResponse};

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
        let Some(content_type) = req.content_type() else {
            return Err(("content-type is missing", Status::BadRequest));
        };

        match content_type::parse(&content_type) {
            (Some(content_type), Some(charset)) if content_type == "application/json" => {
                match charset.to_lowercase().as_str() {
                    "utf-8" | "us-ascii" => serde_json::from_slice::<T>(&req.body)
                        .map(Self)
                        .map_err(|_| {
                            //TODO: Log error back to runtime
                            ("invalid json", Status::BadRequest)
                        }),
                    _ => Err(("invaid charset, expecting `utf-8`", Status::BadRequest)),
                }
            }
            _ => Err((
                "invalid content-type, expecting `application/json; charset=utf-8`",
                Status::BadRequest,
            )),
        }
    }
}

impl<'a, T: Serialize> IntoResponse<'a> for Json<T> {
    fn into_response(self) -> Response<'a> {
        match serde_json::to_vec(&self.0) {
            Ok(vec) => Response::builder()
                .content_type(Cow::Borrowed("application/json; charset=utf-8"))
                .body_from_vec(vec),

            //TODO: log the error back to runtime
            Err(_) => Status::InternalServerError.into_response(),
        }
    }
}
