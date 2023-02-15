// Mostly copied from types in `reqwest` crate
//
// TODO
// 1. Use url type
// 2. Use header type

mod utils;
mod version;

use std::{borrow::Cow, fmt, time::Duration};

use musdk_common::{Header, HttpMethod};

use serde::Serialize;
use version::Version;

//TODO: Use concrete type
pub type Url = String;
pub type Body<'a> = Cow<'a, [u8]>;

const AUTHORIZATION_HEADER: &str = "AUTHORIZATION";
const CONTENT_TYPE_HEADER: &str = "CONTENT_TYPE";

#[derive(Default, Clone)]
pub struct HttpClient;

impl HttpClient {
    pub fn new() -> Self {
        Self
    }

    pub fn get(url: Url) {}
}

/// A request which can be executed with `HttpClient::execute()`.
pub struct HttpRequest<'a> {
    method: HttpMethod,
    url: String,
    headers: Vec<Header<'a>>,
    body: Option<Body<'a>>,
    timeout: Option<Duration>,
    version: Version,
}

/// A builder to construct the properties of a `HttpRequest`.
#[must_use = "HttpRequestBuilder does nothing until you 'send' it"]
pub struct HttpRequestBuilder<'a> {
    client: HttpClient,
    request: Result<HttpRequest<'a>, ()>,
}

impl<'a> HttpRequest<'a> {
    /// Constructs a new request.
    #[inline]
    pub fn new(method: HttpMethod, url: Url) -> Self {
        HttpRequest {
            method,
            url,
            headers: vec![],
            body: None,
            timeout: None,
            version: Version::default(),
        }
    }

    /// Get the method.
    #[inline]
    pub fn method(&self) -> &HttpMethod {
        &self.method
    }

    /// Get the url.
    #[inline]
    pub fn url(&self) -> &Url {
        &self.url
    }

    /// Get the headers.
    #[inline]
    pub fn headers(&self) -> &Vec<Header<'a>> {
        &self.headers
    }

    /// Get the body.
    #[inline]
    pub fn body(&self) -> Option<&Body<'a>> {
        self.body.as_ref()
    }

    /// Get the timeout.
    #[inline]
    pub fn timeout(&self) -> Option<&Duration> {
        self.timeout.as_ref()
    }

    /// Get the http version.
    #[inline]
    pub fn version(&self) -> Version {
        self.version
    }

    pub(super) fn pieces(
        self,
    ) -> (
        HttpMethod,
        Url,
        Vec<Header<'a>>,
        Option<Body<'a>>,
        Option<Duration>,
        Version,
    ) {
        (
            self.method,
            self.url,
            self.headers,
            self.body,
            self.timeout,
            self.version,
        )
    }
}

impl<'a> HttpRequestBuilder<'a> {
    pub(super) fn new(client: HttpClient, request: Result<HttpRequest<'a>, ()>) -> Self {
        let mut builder = HttpRequestBuilder { client, request };

        let auth = builder
            .request
            .as_mut()
            .ok()
            .and_then(|req| extract_authority(&mut req.url));

        if let Some((username, password)) = auth {
            builder.basic_auth(username, password)
        } else {
            builder
        }
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
    pub fn headers(mut self, headers: Vec<Header<'a>>) -> Self {
        if let Ok(ref mut req) = self.request {
            req.headers.append(&mut headers);
        }
        self
    }

    /// Enable HTTP basic authentication.
    pub fn basic_auth<U, P>(self, username: U, password: Option<P>) -> Self
    where
        U: fmt::Display,
        P: fmt::Display,
    {
        let header_value = utils::basic_auth(username, password);
        self.header(AUTHORIZATION_HEADER, header_value)
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
            req.body = Some(body.into());
        }
        self
    }

    /// Enables a request timeout.
    ///
    /// The timeout is applied from when the request starts connecting until the
    /// response body has finished.
    pub fn timeout(mut self, timeout: Duration) -> Self {
        if let Ok(ref mut req) = self.request {
            req.timeout = Some(timeout);
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
    //pub fn query<T: Serialize + ?Sized>(mut self, query: &T) -> HttpRequestBuilder {
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
    pub fn form<T: Serialize + ?Sized>(mut self, form: &T) -> HttpRequestBuilder {
        let mut error = None;
        if let Ok(ref mut req) = self.request {
            match serde_urlencoded::to_string(form) {
                Ok(body) => {
                    req.headers.push(Header {
                        name: CONTENT_TYPE_HEADER.into(),
                        value: "application/x-www-form-urlencoded".into(),
                    });

                    req.body = Some(body.into());
                }
                Err(err) => error = Some(crate::error::builder(err)), //TODO
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
                    req.body = Some(body.into());
                }
                Err(err) => error = Some(crate::error::builder(err)), //TODO
            }
        }
        if let Some(err) = error {
            self.request = Err(err);
        }
        self
    }

    /// Build a `Request`, which can be inspected, modified and executed with
    /// `HttpClient::execute()`.
    pub fn build(self) -> Result<HttpRequest<'a>, ()> {
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
    pub fn send(self) -> Result<HttpResponse, Error> {
        self.request.map(self.client.execute_request)
    }

    /// Clone the HttpRequestBuilder.
    pub fn clone(&self) -> Option<Self> {
        self.request
            .as_ref()
            .ok()
            .map(|req| req.clone())
            .map(|req| HttpRequestBuilder {
                client: self.client.clone(),
                request: Ok(req),
            })
    }
}

impl<'a> fmt::Debug for HttpRequest<'a> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        fmt_request_fields(&mut f.debug_struct("Request"), self).finish()
    }
}

impl<'a> fmt::Debug for HttpRequestBuilder<'a> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let mut builder = f.debug_struct("HttpRequestBuilder");
        match self.request {
            Ok(ref req) => fmt_request_fields(&mut builder, req).finish(),
            Err(ref err) => builder.field("error", err).finish(),
        }
    }
}

fn fmt_request_fields<'a, 'b>(
    f: &'a mut fmt::DebugStruct<'a, 'b>,
    req: &HttpRequest,
) -> &'a mut fmt::DebugStruct<'a, 'b> {
    f.field("method", &req.method)
        .field("url", &req.url)
        .field("headers", &req.headers)
}

/// Check the request URL for a "username:password" type authority, and if
/// found, remove it from the URL and return it.
pub(crate) fn extract_authority(url: &mut Url) -> Option<(String, Option<String>)> {
    use percent_encoding::percent_decode;

    if url.has_authority() {
        let username: String = percent_decode(url.username().as_bytes())
            .decode_utf8()
            .ok()?
            .into();
        let password = url.password().and_then(|pass| {
            percent_decode(pass.as_bytes())
                .decode_utf8()
                .ok()
                .map(String::from)
        });
        if !username.is_empty() || password.is_some() {
            url.set_username("")
                .expect("has_authority means set_username shouldn't fail");
            url.set_password(None)
                .expect("has_authority means set_password shouldn't fail");
            return Some((username, password));
        }
    }

    None
}
