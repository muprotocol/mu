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

use std::fmt;

use borsh::{BorshDeserialize, BorshSerialize};

#[derive(Debug, Clone, Copy)]
pub struct Status {
    pub code: u16,
}

impl Default for Status {
    fn default() -> Self {
        Status::Ok
    }
}

macro_rules! create_status {
    ($($code:expr, $code_str:expr, $name:ident => $reason:expr),+) => {
        $(
            #[doc="[`Status`] with code <b>"]
            #[doc=$code_str]
            #[doc="</b>."]
            #[allow(non_upper_case_globals)]
            pub const $name: Status = Status { code: $code };
        )+

        /// Creates a new `Status` with `code`. This should be used _only_ to
        /// construct non-standard HTTP statuses. Use an associated constant for
        /// standard statuses.
        pub const fn new(code: u16) -> Status {
            Status { code }
        }

        /// Returns a Status given a standard status code `code`. If `code` is
        /// not a known status code, `None` is returned.
        pub const fn from_code(code: u16) -> Option<Status> {
            match code {
                $($code => Some(Status::$name),)+
                _ => None
            }
        }

        /// Returns the canonical reason phrase if `self` corresponds to a
        /// canonical, known status code. Otherwise, returns `None`.
        pub const fn reason(&self) -> Option<&'static str> {
            match self.code {
                $($code => Some($reason),)+
                _ => None
            }
        }


        /// Returns the canonical reason phrase if `self` corresponds to a
        /// canonical, known status code, or an unspecified but relevant reason
        /// phrase otherwise.
        pub const fn reason_lossy(&self) -> &'static str {
            if let Some(lossless) = self.reason() {
                return lossless;
            }

            match self.code % 100 {
                1 => "Informational",
                2 => "Success",
                3 => "Redirection",
                4 => "Client Error",
                5 => "Server Error",
                _ => "Unknown"
            }
        }

        pub const fn is_client_error(&self) -> bool {
            self.code % 100 == 4
        }
    };
}

#[allow(dead_code)]
impl Status {
    create_status! {
        100, "100", Continue => "Continue",
        101, "101", SwitchingProtocols => "Switching Protocols",
        102, "102", Processing => "Processing",
        200, "200", Ok => "OK",
        201, "201", Created => "Created",
        202, "202", Accepted => "Accepted",
        203, "203", NonAuthoritativeInformation => "Non-Authoritative Information",
        204, "204", NoContent => "No Content",
        205, "205", ResetContent => "Reset Content",
        206, "206", PartialContent => "Partial Content",
        207, "207", MultiStatus => "Multi-Status",
        208, "208", AlreadyReported => "Already Reported",
        226, "226", ImUsed => "IM Used",
        300, "300", MultipleChoices => "Multiple Choices",
        301, "301", MovedPermanently => "Moved Permanently",
        302, "302", Found => "Found",
        303, "303", SeeOther => "See Other",
        304, "304", NotModified => "Not Modified",
        305, "305", UseProxy => "Use Proxy",
        307, "307", TemporaryRedirect => "Temporary Redirect",
        308, "308", PermanentRedirect => "Permanent Redirect",
        400, "400", BadRequest => "Bad Request",
        401, "401", Unauthorized => "Unauthorized",
        402, "402", PaymentRequired => "Payment Required",
        403, "403", Forbidden => "Forbidden",
        404, "404", NotFound => "Not Found",
        405, "405", MethodNotAllowed => "Method Not Allowed",
        406, "406", NotAcceptable => "Not Acceptable",
        407, "407", ProxyAuthenticationRequired => "Proxy Authentication Required",
        408, "408", RequestTimeout => "Request Timeout",
        409, "409", Conflict => "Conflict",
        410, "410", Gone => "Gone",
        411, "411", LengthRequired => "Length Required",
        412, "412", PreconditionFailed => "Precondition Failed",
        413, "413", PayloadTooLarge => "Payload Too Large",
        414, "414", UriTooLong => "URI Too Long",
        415, "415", UnsupportedMediaType => "Unsupported Media Type",
        416, "416", RangeNotSatisfiable => "Range Not Satisfiable",
        417, "417", ExpectationFailed => "Expectation Failed",
        418, "418", ImATeapot => "I'm a teapot",
        421, "421", MisdirectedRequest => "Misdirected Request",
        422, "422", UnprocessableEntity => "Unprocessable Entity",
        423, "423", Locked => "Locked",
        424, "424", FailedDependency => "Failed Dependency",
        426, "426", UpgradeRequired => "Upgrade Required",
        428, "428", PreconditionRequired => "Precondition Required",
        429, "429", TooManyRequests => "Too Many Requests",
        431, "431", RequestHeaderFieldsTooLarge => "Request Header Fields Too Large",
        451, "451", UnavailableForLegalReasons => "Unavailable For Legal Reasons",
        500, "500", InternalServerError => "Internal Server Error",
        501, "501", NotImplemented => "Not Implemented",
        502, "502", BadGateway => "Bad Gateway",
        503, "503", ServiceUnavailable => "Service Unavailable",
        504, "504", GatewayTimeout => "Gateway Timeout",
        505, "505", HttpVersionNotSupported => "HTTP Version Not Supported",
        506, "506", VariantAlsoNegotiates => "Variant Also Negotiates",
        507, "507", InsufficientStorage => "Insufficient Storage",
        508, "508", LoopDetected => "Loop Detected",
        510, "510", NotExtended => "Not Extended",
        511, "511", NetworkAuthenticationRequired => "Network Authentication Required"
    }
}

impl fmt::Display for Status {
    #[inline(always)]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} {}", self.code, self.reason_lossy())
    }
}

impl std::hash::Hash for Status {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.code.hash(state)
    }
}

impl PartialEq for Status {
    fn eq(&self, other: &Self) -> bool {
        self.code.eq(&other.code)
    }
}

impl Eq for Status {}

impl PartialOrd for Status {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        self.code.partial_cmp(&other.code)
    }
}

impl Ord for Status {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.code.cmp(&other.code)
    }
}

impl BorshSerialize for Status {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        <u16 as BorshSerialize>::serialize(&self.code, writer)
    }
}

impl BorshDeserialize for Status {
    fn deserialize_reader<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        <u16 as BorshDeserialize>::deserialize_reader(reader).map(|code| Self { code })
    }
}
