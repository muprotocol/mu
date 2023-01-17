use std::{borrow::Cow, collections::HashMap};

use musdk_common::{Request, Status};

use crate::{content_type, IntoResponse};

pub trait FromRequest<'a>: Sized {
    type Error: IntoResponse<'static>;

    fn from_request(req: &'a Request) -> Result<Self, Self::Error>;
}

impl<'a> FromRequest<'a> for &'a Request<'a> {
    type Error = ();

    fn from_request(req: &'a Request) -> Result<Self, Self::Error> {
        Ok(req)
    }
}

impl<'a> FromRequest<'a> for &'a [u8] {
    type Error = ();

    fn from_request(req: &'a Request) -> Result<Self, ()> {
        Ok(&req.body)
    }
}

impl<'a> FromRequest<'a> for Vec<u8> {
    type Error = ();

    fn from_request(req: &'a Request) -> Result<Self, ()> {
        Ok(req.body.to_vec())
    }
}

impl<'a> FromRequest<'a> for &'a str {
    //TODO: concrete error type
    type Error = (String, Status);

    fn from_request(req: &'a Request) -> Result<Self, Self::Error> {
        let content_type = req.content_type();
        let charset = content_type
            .as_ref()
            .and_then(|s| content_type::parse(s).1)
            .unwrap_or("us-ascii");

        match charset.to_lowercase().as_str() {
            "utf-8" | "us-ascii" => {
                core::str::from_utf8(&req.body).map_err(|e| (e.to_string(), Status::BadRequest))
            }
            ch => Err((format!("unsupported charset: {ch}"), Status::BadRequest)),
        }
    }
}

impl<'a> FromRequest<'a> for String {
    type Error = <&'a str as FromRequest<'a>>::Error;

    fn from_request(req: &'a Request) -> Result<Self, Self::Error> {
        <&'a str as FromRequest<'a>>::from_request(req).map(ToString::to_string)
    }
}

pub struct PathParams<'a>(HashMap<Cow<'a, str>, Cow<'a, str>>);
pub struct QueryParams<'a>(HashMap<Cow<'a, str>, Cow<'a, str>>);

impl<'a> FromRequest<'a> for PathParams<'a> {
    type Error = ();

    fn from_request(req: &'a Request) -> Result<Self, Self::Error> {
        let map = req
            .path_params
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();

        Ok(Self(map))
    }
}

impl<'a> FromRequest<'a> for QueryParams<'a> {
    type Error = ();

    fn from_request(req: &'a Request) -> Result<Self, Self::Error> {
        let map = req
            .query_params
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();

        Ok(Self(map))
    }
}
