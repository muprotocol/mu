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

use super::Status;

/// The Errors that may occur when processing an `Request`
#[derive(Debug, BorshSerialize, BorshDeserialize)]
pub enum Error {
    Builder(String),
    Request(String),
    Redirect(String),
    Status(Status),
    Body(String),
    Decode(String),
    Upgrade(String),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Error::Builder(e) => f.write_fmt(format_args!("builder error: {e:?}"))?,
            Error::Request(e) => f.write_fmt(format_args!("error sending request: {e:?}"))?,
            Error::Body(e) => f.write_fmt(format_args!("request or response body error: {e:?}"))?,
            Error::Decode(e) => f.write_fmt(format_args!("error decoding response body: {e:?}"))?,
            Error::Redirect(e) => f.write_fmt(format_args!("error following redirect {e:?}"))?,
            Error::Upgrade(e) => f.write_fmt(format_args!("error upgrading connection {e:?}"))?,
            Error::Status(ref status) => {
                let prefix = if status.is_client_error() {
                    "HTTP status client error"
                } else {
                    "HTTP status server error"
                };
                write!(f, "{prefix} ({status})")?;
            }
        };

        Ok(())
    }
}
