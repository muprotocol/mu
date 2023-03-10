use std::{borrow::Cow, error::Error};

use log::error;
use musdk_common::http_client::{self, *};
use reqwest::Method;

pub fn http_method_to_reqwest_method(method: HttpMethod) -> reqwest::Method {
    match method {
        HttpMethod::Get => Method::GET,
        HttpMethod::Head => Method::HEAD,
        HttpMethod::Post => Method::POST,
        HttpMethod::Put => Method::PUT,
        HttpMethod::Patch => Method::PATCH,
        HttpMethod::Delete => Method::DELETE,
        HttpMethod::Options => Method::OPTIONS,
    }
}

pub fn version_to_reqwest_version(version: Version) -> reqwest::Version {
    match version {
        Version::HTTP_09 => reqwest::Version::HTTP_09,
        Version::HTTP_10 => reqwest::Version::HTTP_10,
        Version::HTTP_11 => reqwest::Version::HTTP_11,
        Version::HTTP_2 => reqwest::Version::HTTP_2,
        Version::HTTP_3 => reqwest::Version::HTTP_3,
    }
}

fn error_reason(error: reqwest::Error) -> String {
    error
        .source()
        .map(ToString::to_string)
        .unwrap_or("".to_string())
}

pub fn reqwest_error_to_http_error(error: reqwest::Error) -> http_client::Error {
    if error.is_builder() {
        http_client::Error::Builder(error_reason(error))
    } else if error.is_request() {
        http_client::Error::Request(error_reason(error))
    } else if error.is_redirect() {
        http_client::Error::Redirect(error_reason(error))
    } else if error.is_status() {
        // Note: this should not happen and we safely map unknown statuses to 200
        let status = Status::from_code(error.status().map(|s| s.as_u16()).unwrap_or(200))
            .unwrap_or(Status::default());
        http_client::Error::Status(status)
    } else if error.is_body() {
        http_client::Error::Body(error_reason(error))
    } else if error.is_decode() {
        http_client::Error::Decode(error_reason(error))
    } else {
        http_client::Error::Upgrade(error_reason(error))
    }
}

pub fn reqwest_response_to_http_response<'a>(
    response: reqwest::Result<reqwest::blocking::Response>,
) -> Result<Response<'a>, http_client::Error> {
    let response = response.map_err(reqwest_error_to_http_error)?;

    let status = Status::from_code(response.status().as_u16()).unwrap_or(Status::default());

    let headers = response
            .headers()
            .clone() //TODO: Maybe not?
            .into_iter()
            .map(|(name, value)| -> Result<Header, http_client::Error> {
                let Some(name) = name else {return Err(http_client::Error::Decode("invalid header with empty name".to_string()))};

                let value = value.to_str().map_err(|e| {
                    error!("invalid header value in http response: {e:?}");
                    http_client::Error::Decode("invalid header value".to_string())
                })?;

                Ok(Header {
                    name: Cow::Owned(name.as_str().to_string()),
                    value: Cow::Owned(value.to_string()),
                })
            })
            .collect::<Result<Vec<Header>, _>>()?;

    let body = response
        .bytes()
        .map_err(reqwest_error_to_http_error)?
        .to_vec();

    Ok(Response::builder()
        .status(status)
        .headers(headers)
        .body_from_vec(body))
}
