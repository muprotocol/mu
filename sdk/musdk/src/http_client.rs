// parts of this file are derived from `reqwest` https://github.com/seanmonstar/reqwest
//
// Copyright (c) 2016 Sean McArthur
//
// Permission is hereby granted, free of charge, to any person obtaining a copy
// of this software and associated documentation files (the "Software"), to deal
// in the Software without restriction, including without limitation the rights
// to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
// copies of the Software, and to permit persons to whom the Software is
// furnished to do so, subject to the following conditions:
//
// The above copyright notice and this permission notice shall be included in
// all copies or substantial portions of the Software.
//
// THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
// IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
// FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
// AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
// LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
// OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN
// THE SOFTWARE.

use std::{borrow::Cow, fmt};

use musdk_common::{
    http_client::{
        header::{AUTHORIZATION_HEADER, CONTENT_TYPE_HEADER},
        *,
    },
    incoming_message::IncomingMessage,
    outgoing_message::OutgoingMessage,
};

use serde::Serialize;

use crate::{error, MuContext};

pub struct HttpClient<'c> {
    ctx: &'c mut MuContext,
}

impl<'c> HttpClient<'c> {
    /// Send request to runtime and receive the response.
    pub fn execute_request(
        &mut self,
        req: Request,
    ) -> Result<Result<Response<'static>, Error>, ClientError> {
        self.ctx
            .write_message(OutgoingMessage::HttpRequest(req))
            .map_err(ClientError::SendRequest)?;

        match self.ctx.read_message().map_err(ClientError::RecvResponse)? {
            IncomingMessage::HttpResponse(response) => Ok(response),
            _ => Err(ClientError::RecvResponse(
                error::Error::UnexpectedMessageKind("HttpResponse"),
            )),
        }
    }

    /// Convenience method to make a `GET` request to a URL.
    pub fn get<S: Into<String>>(&mut self, url: S) -> RequestBuilder {
        self.request(HttpMethod::Get, url.into())
    }

    /// Convenience method to make a `POST` request to a URL.
    pub fn post<S: Into<String>>(&mut self, url: S) -> RequestBuilder {
        self.request(HttpMethod::Post, url.into())
    }

    /// Convenience method to make a `PUT` request to a URL.
    pub fn put<S: Into<String>>(&mut self, url: S) -> RequestBuilder {
        self.request(HttpMethod::Put, url.into())
    }

    /// Convenience method to make a `PATCH` request to a URL.
    pub fn patch<S: Into<String>>(&mut self, url: S) -> RequestBuilder {
        self.request(HttpMethod::Patch, url.into())
    }

    /// Convenience method to make a `DELETE` request to a URL.
    pub fn delete<S: Into<String>>(&mut self, url: S) -> RequestBuilder {
        self.request(HttpMethod::Delete, url.into())
    }

    /// Convenience method to make a `HEAD` request to a URL.
    pub fn head<S: Into<String>>(&mut self, url: S) -> RequestBuilder {
        self.request(HttpMethod::Head, url.into())
    }

    /// Start building a `Request` with the `HttpMethod` and `Url`.
    ///
    /// Returns a `RequestBuilder`, which will allow setting headers and
    /// the request body before sending.
    pub fn request<S: Into<String>>(&mut self, method: HttpMethod, url: S) -> RequestBuilder {
        let req = Request::new(method, url.into());

        let client = HttpClient { ctx: self.ctx };
        RequestBuilder::new(client, Ok(req))
    }

    /// Executes a `Request`.
    ///
    /// A `Request` can be built manually with `Request::new()` or obtained
    /// from a RequestBuilder with `RequestBuilder::build()`.
    ///
    /// You should prefer to use the `RequestBuilder` and
    /// `RequestBuilder::send()`.
    ///
    /// # Errors
    ///
    /// This method fails if there was an error while sending request,
    /// redirect loop was detected or redirect limit was exhausted.
    pub fn execute(&mut self, request: Request) -> Result<Result<Response, Error>, ClientError> {
        self.execute_request(request)
    }

    #[inline]
    pub(crate) fn new(ctx: &'c mut MuContext) -> Self {
        Self { ctx }
    }
}

/// A builder to construct the properties of a `Request`.
#[must_use = "RequestBuilder does nothing until you 'send' it"]
pub struct RequestBuilder<'a, 'c: 'a> {
    client: HttpClient<'c>,
    request: Result<Request<'a>, Error>,
}

impl<'a, 'c: 'a> RequestBuilder<'a, 'c> {
    pub(super) fn new(client: HttpClient<'c>, request: Result<Request<'a>, Error>) -> Self {
        RequestBuilder { client, request }
    }

    /// Add a `Header` to this Request.
    fn header<K, V>(mut self, name: K, value: V) -> Self
    where
        Cow<'a, str>: From<K>,
        Cow<'a, str>: From<V>,
    {
        if let Ok(ref mut req) = self.request {
            req.headers.push(Header {
                name: name.into(),
                value: value.into(),
            });
        }
        self
    }

    /// Add a set of Headers to the existing ones on this Request.
    ///
    /// The headers will be merged in to any already set.
    pub fn headers(mut self, mut headers: Vec<Header<'a>>) -> Self {
        if let Ok(ref mut req) = self.request {
            req.headers.append(&mut headers);
        }
        self
    }

    /// Enable HTTP bearer authentication.
    pub fn bearer_auth<T>(self, token: T) -> Self
    where
        T: fmt::Display,
    {
        let header_value = format!("Bearer {}", token);
        self.header(AUTHORIZATION_HEADER, header_value)
    }

    /// Set the request body.
    pub fn body<T: Into<Body<'a>>>(mut self, body: T) -> Self {
        if let Ok(ref mut req) = self.request {
            req.body = body.into();
        }
        self
    }

    //TODO
    ///// Modify the query string of the URL.
    /////
    ///// Modifies the URL of this request, adding the parameters provided.
    ///// This method appends and does not overwrite. This means that it can
    ///// be called multiple times and that existing query parameters are not
    ///// overwritten if the same key is used. The key will simply show up
    ///// twice in the query string.
    ///// Calling `.query(&[("foo", "a"), ("foo", "b")])` gives `"foo=a&foo=b"`.
    /////
    ///// # Note
    ///// This method does not support serializing a single key-value
    ///// pair. Instead of using `.query(("key", "val"))`, use a sequence, such
    ///// as `.query(&[("key", "val")])`. It's also possible to serialize structs
    ///// and maps into a key-value pair.
    /////
    ///// # Errors
    ///// This method will fail if the object you provide cannot be serialized
    ///// into a query string.
    //pub fn query<T: Serialize + ?Sized>(mut self, query: &T) -> RequestBuilder {
    //    let mut error = None;
    //    if let Ok(ref mut req) = self.request {
    //        let url = req.url_mut();
    //        let mut pairs = url.query_pairs_mut();
    //        let serializer = serde_urlencoded::Serializer::new(&mut pairs);

    //        if let Err(err) = query.serialize(serializer) {
    //            error = Some(crate::error::builder(err));
    //        }
    //    }
    //    if let Ok(ref mut req) = self.request {
    //        if let Some("") = req.url().query() {
    //            req.url_mut().set_query(None);
    //        }
    //    }
    //    if let Some(err) = error {
    //        self.request = Err(err);
    //    }
    //    self
    //}

    /// Set HTTP version
    pub fn version(mut self, version: Version) -> Self {
        if let Ok(ref mut req) = self.request {
            req.version = version;
        }
        self
    }

    /// Send a form body.
    ///
    /// Sets the body to the url encoded serialization of the passed value,
    /// and also sets the `Content-Type: application/x-www-form-urlencoded`
    /// header.
    ///
    /// # Errors
    ///
    /// This method fails if the passed value cannot be serialized into
    /// url encoded format
    pub fn form<T: Serialize + ?Sized>(mut self, form: &'a T) -> RequestBuilder {
        let mut error = None;
        if let Ok(ref mut req) = self.request {
            match serde_urlencoded::to_string(form) {
                Ok(body) => {
                    req.headers.push(Header {
                        name: CONTENT_TYPE_HEADER.into(),
                        value: "application/x-www-form-urlencoded".into(),
                    });

                    req.body = body.into_bytes().into();
                }
                Err(err) => {
                    error = Some(Error::Request(format!("failed to serialize url: {err:?}")))
                }
            }
        }
        if let Some(err) = error {
            self.request = Err(err);
        }
        self
    }

    /// Send a JSON body.
    ///
    /// # Optional
    ///
    /// This requires the optional `json` feature enabled.
    ///
    /// # Errors
    ///
    /// Serialization can fail if `T`'s implementation of `Serialize` decides to
    /// fail, or if `T` contains a map with non-string keys.
    #[cfg(feature = "json")]
    #[cfg_attr(docsrs, doc(cfg(feature = "json")))]
    pub fn json<T: Serialize + ?Sized>(mut self, json: &T) -> Self {
        let mut error = None;
        if let Ok(ref mut req) = self.request {
            match serde_json::to_vec(json) {
                Ok(body) => {
                    req.headers.push(Header {
                        name: CONTENT_TYPE_HEADER.into(),
                        value: "application/json".into(),
                    });
                    req.body = body.into();
                }
                Err(err) => {
                    error = Some(Error::Request(format!(
                        "failed to serialize request: {err:?}"
                    )))
                }
            }
        }
        if let Some(err) = error {
            self.request = Err(err);
        }
        self
    }

    /// Build a `Request`, which can be inspected, modified and executed with
    /// `HttpClient::execute()`.
    pub fn build(self) -> Result<Request<'a>, Error> {
        self.request
    }

    /// Constructs the Request and sends it to the target URL, returning a
    /// future Response.
    ///
    /// # Errors
    ///
    /// This method fails if there was an error while sending request,
    /// redirect loop was detected or redirect limit was exhausted.
    ///
    pub fn send(mut self) -> Result<Result<Response<'static>, Error>, ClientError> {
        match self.request {
            Ok(req) => self.client.execute_request(req),
            Err(e) => Ok(Err(e)),
        }
    }
}

impl<'a> fmt::Debug for RequestBuilder<'a, '_> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let mut builder = f.debug_struct("RequestBuilder");
        match self.request {
            Ok(ref req) => fmt_request_fields(&mut builder, req).finish(),
            Err(ref err) => builder.field("error", err).finish(),
        }
    }
}

fn fmt_request_fields<'a, 'b>(
    f: &'a mut fmt::DebugStruct<'a, 'b>,
    req: &Request,
) -> &'a mut fmt::DebugStruct<'a, 'b> {
    f.field("method", &req.method)
        .field("url", &req.url)
        .field("headers", &req.headers)
}

#[derive(Debug)]
pub enum ClientError {
    SendRequest(error::Error),
    RecvResponse(error::Error),
}
