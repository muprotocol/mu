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

impl<'a> FromRequest<'a> for String {
    //TODO: concrete error type
    type Error = (String, Status);

    fn from_request(req: &'a Request) -> Result<Self, Self::Error> {
        let content_type_header_index = req.headers.iter().enumerate().find_map(|(i, header)| {
            if &header.name.to_lowercase() == "content-type" {
                Some(i)
            } else {
                None
            }
        });

        let (content_type, charset) = match content_type_header_index {
            None => ("text/plain", "us-ascii"),
            Some(index) => {
                let header = &req.headers[index];
                let (content_type, charset) = content_type::parse(&header.value);
                (
                    content_type.unwrap_or("text/plain"),
                    charset.unwrap_or("us-ascii"),
                )
            }
        };

        if let Some("text") = content_type.split('/').next() {
            match charset.to_lowercase().as_str() {
                "utf-8" | "us-ascii" => String::from_utf8(req.body.to_vec())
                    .map_err(|e| (e.to_string(), Status::BadRequest)),

                ch => Err((
                    format!("unsupported text charset: {ch}"),
                    Status::BadRequest,
                )),
            }
        } else {
            Err((
                "can not parse request as string, content-type is not `text`".into(),
                Status::BadRequest,
            ))
        }
    }
}

impl<'a> FromRequest<'a> for &'a str {
    //TODO: concrete error type
    type Error = (String, Status);

    fn from_request(req: &'a Request) -> Result<Self, Self::Error> {
        let content_type_header_index = req.headers.iter().enumerate().find_map(|(i, header)| {
            if &header.name.to_lowercase() == "content-type" {
                Some(i)
            } else {
                None
            }
        });

        let (content_type, charset) = match content_type_header_index {
            None => ("text/plain", "us-ascii"),
            Some(index) => {
                let header = &req.headers[index];
                let (content_type, charset) = content_type::parse(&header.value);
                (
                    content_type.unwrap_or("text/plain"),
                    charset.unwrap_or("us-ascii"),
                )
            }
        };

        if let Some("text") = content_type.split('/').next() {
            match charset.to_lowercase().as_str() {
                "utf-8" | "us-ascii" => {
                    core::str::from_utf8(&req.body).map_err(|e| (e.to_string(), Status::BadRequest))
                }

                ch => Err((
                    format!("unsupported text charset: {ch}"),
                    Status::BadRequest,
                )),
            }
        } else {
            Err((
                "can not parse request as string, content-type is not `text`".into(),
                Status::BadRequest,
            ))
        }
    }
}
