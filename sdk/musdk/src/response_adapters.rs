use musdk_common::{Response, Status};

pub trait IntoResponse<'a> {
    fn into_response(self) -> Response<'a>;
}

impl<'a> IntoResponse<'a> for Response<'a> {
    fn into_response(self) -> Response<'a> {
        self
    }
}

impl<'a> IntoResponse<'a> for () {
    fn into_response(self) -> Response<'a> {
        Response::builder().body_from_slice(&[])
    }
}

impl<'a, T, E> IntoResponse<'a> for Result<T, E>
where
    T: IntoResponse<'a>,
    E: IntoResponse<'a>,
{
    fn into_response(self) -> Response<'a> {
        match self {
            Ok(r) => r.into_response(),
            Err(r) => r.into_response(),
        }
    }
}

/// Override status code of the `T`
impl<'a, T> IntoResponse<'a> for (T, Status)
where
    T: IntoResponse<'a>,
{
    fn into_response(self) -> Response<'a> {
        let mut resp = self.0.into_response();
        resp.status = self.1;
        resp
    }
}

impl<'a> IntoResponse<'a> for &'a [u8] {
    fn into_response(self) -> Response<'a> {
        Response::builder().body_from_slice(self)
    }
}

impl<'a> IntoResponse<'a> for Vec<u8> {
    fn into_response(self) -> Response<'a> {
        Response::builder().body_from_vec(self)
    }
}

impl<'a> IntoResponse<'a> for &'a str {
    fn into_response(self) -> Response<'a> {
        Response::builder().body_from_str(self)
    }
}

impl<'a> IntoResponse<'a> for String {
    fn into_response(self) -> Response<'a> {
        Response::builder().body_from_string(self)
    }
}

impl<'a> IntoResponse<'a> for Status {
    fn into_response(self) -> Response<'a> {
        Response::builder().status(self).no_body()
    }
}
