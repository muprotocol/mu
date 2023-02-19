use std::borrow::Cow;

use anyhow::{anyhow, bail, Context, Result};
use mu_stack::StackID;
use musdk_common::{Status, Version};
use protobuf::{EnumOrUnknown, MessageField};

include!(concat!(env!("OUT_DIR"), "/protos/rpc/mod.rs"));

impl From<mu_stack::FunctionID> for rpc::FunctionID {
    fn from(id: mu_stack::FunctionID) -> Self {
        let StackID::SolanaPublicKey(pk) = id.assembly_id.stack_id;
        Self {
            stack_id: MessageField(Some(Box::new(rpc::StackID {
                id: Some(rpc::stack_id::Id::Solana(pk.into())),
                ..Default::default()
            }))),
            assembly_name: id.assembly_id.assembly_name,
            function_name: id.function_name,
            ..Default::default()
        }
    }
}

impl TryFrom<rpc::FunctionID> for mu_stack::FunctionID {
    type Error = anyhow::Error;

    fn try_from(id: rpc::FunctionID) -> Result<Self, Self::Error> {
        let stack_id = id.stack_id.0.ok_or_else(|| anyhow!("Empty stack ID"))?;
        let stack_id = match stack_id.id {
            Some(rpc::stack_id::Id::Solana(bytes)) => StackID::SolanaPublicKey(
                bytes
                    .try_into()
                    .map_err(|_| anyhow!("Incorrect stack ID length"))?,
            ),

            None => bail!("Empty stack ID"),
        };

        Ok(Self {
            assembly_id: mu_stack::AssemblyID {
                stack_id,
                assembly_name: id.assembly_name,
            },
            function_name: id.function_name,
        })
    }
}

fn header_to_proto(h: musdk_common::Header<'_>) -> rpc::KeyValuePair {
    rpc::KeyValuePair {
        key: h.name.into_owned(),
        value: h.value.into_owned(),
        ..Default::default()
    }
}

fn header_from_proto(h: rpc::KeyValuePair) -> musdk_common::Header<'static> {
    musdk_common::Header {
        name: Cow::Owned(h.key),
        value: Cow::Owned(h.value),
    }
}

impl<'a> From<musdk_common::Request<'a>> for rpc::Request {
    fn from(request: musdk_common::Request<'a>) -> Self {
        // we have the same code in the mu_stack crate as well. We could
        // unify the two sources if we set up a really complex scenario in which
        // the proto files in this crate reference those of the mu_stack crate,
        // and then we'd have to make the codegen look in the other crate's code,
        // but it doesn't seem to want to do this. All in all, not worth it IMO.
        fn convert_http_method(method: musdk_common::HttpMethod) -> EnumOrUnknown<rpc::HttpMethod> {
            let value = match method {
                musdk_common::HttpMethod::Get => rpc::HttpMethod::GET,
                musdk_common::HttpMethod::Post => rpc::HttpMethod::POST,
                musdk_common::HttpMethod::Patch => rpc::HttpMethod::PATCH,
                musdk_common::HttpMethod::Put => rpc::HttpMethod::PUT,
                musdk_common::HttpMethod::Delete => rpc::HttpMethod::DELETE,
                musdk_common::HttpMethod::Head => rpc::HttpMethod::HEAD,
                musdk_common::HttpMethod::Options => rpc::HttpMethod::OPTIONS,
            };

            EnumOrUnknown::new(value)
        }

        fn convert_key_value_pair<'a>(p: (Cow<'a, str>, Cow<'a, str>)) -> rpc::KeyValuePair {
            let (k, v) = p;
            rpc::KeyValuePair {
                key: k.into_owned(),
                value: v.into_owned(),
                ..Default::default()
            }
        }

        fn convert_version(v: Version) -> EnumOrUnknown<rpc::HttpVersion> {
            let value = match v {
                Version::HTTP_09 => rpc::HttpVersion::HTTP09,
                Version::HTTP_10 => rpc::HttpVersion::HTTP10,
                Version::HTTP_11 => rpc::HttpVersion::HTTP11,
                Version::HTTP_2 => rpc::HttpVersion::HTTP2,
                Version::HTTP_3 => rpc::HttpVersion::HTTP3,
                _ => unreachable!(),
            };

            EnumOrUnknown::new(value)
        }

        Self {
            url: request.url,
            method: convert_http_method(request.method),
            path_params: request
                .path_params
                .into_iter()
                .map(convert_key_value_pair)
                .collect(),
            query_params: request
                .query_params
                .into_iter()
                .map(convert_key_value_pair)
                .collect(),
            headers: request.headers.into_iter().map(header_to_proto).collect(),
            body: request.body.into_owned(),
            http_version: convert_version(request.version),
            ..Default::default()
        }
    }
}

impl TryFrom<rpc::Request> for musdk_common::Request<'static> {
    type Error = anyhow::Error;

    fn try_from(request: rpc::Request) -> Result<Self> {
        fn convert_http_method(
            method: EnumOrUnknown<rpc::HttpMethod>,
        ) -> Result<musdk_common::HttpMethod> {
            method
                .enum_value()
                .map(|e| match e {
                    rpc::HttpMethod::GET => musdk_common::HttpMethod::Get,
                    rpc::HttpMethod::POST => musdk_common::HttpMethod::Post,
                    rpc::HttpMethod::PATCH => musdk_common::HttpMethod::Patch,
                    rpc::HttpMethod::PUT => musdk_common::HttpMethod::Put,
                    rpc::HttpMethod::DELETE => musdk_common::HttpMethod::Delete,
                    rpc::HttpMethod::HEAD => musdk_common::HttpMethod::Head,
                    rpc::HttpMethod::OPTIONS => musdk_common::HttpMethod::Options,
                })
                .map_err(|i| anyhow!("Unknown enum value {i} for type HttpMethod"))
        }

        fn convert_key_value(p: rpc::KeyValuePair) -> (Cow<'static, str>, Cow<'static, str>) {
            (Cow::Owned(p.key), Cow::Owned(p.value))
        }

        fn header_from_proto(h: rpc::KeyValuePair) -> musdk_common::Header<'static> {
            musdk_common::Header {
                name: Cow::Owned(h.key),
                value: Cow::Owned(h.value),
            }
        }

        fn version_from_proto(v: EnumOrUnknown<rpc::HttpVersion>) -> Result<musdk_common::Version> {
            v.enum_value()
                .map(|v| match v {
                    rpc::HttpVersion::HTTP09 => musdk_common::Version::HTTP_09,
                    rpc::HttpVersion::HTTP10 => musdk_common::Version::HTTP_10,
                    rpc::HttpVersion::HTTP11 => musdk_common::Version::HTTP_11,
                    rpc::HttpVersion::HTTP2 => musdk_common::Version::HTTP_2,
                    rpc::HttpVersion::HTTP3 => musdk_common::Version::HTTP_3,
                })
                .map_err(|i| anyhow!("Unknown enum value {i} for type HttpVersion"))
        }

        Ok(Self {
            method: convert_http_method(request.method)?,
            url: request.url,
            path_params: request
                .path_params
                .into_iter()
                .map(convert_key_value)
                .collect(),
            query_params: request
                .query_params
                .into_iter()
                .map(convert_key_value)
                .collect(),
            headers: request.headers.into_iter().map(header_from_proto).collect(),
            body: Cow::Owned(request.body),
            version: version_from_proto(request.http_version)?,
        })
    }
}

impl<'a> From<musdk_common::Response<'a>> for rpc::Response {
    fn from(response: musdk_common::Response<'a>) -> Self {
        Self {
            status: response.status.code as i32,
            headers: response.headers.into_iter().map(header_to_proto).collect(),
            body: response.body.into_owned(),
            ..Default::default()
        }
    }
}

impl TryFrom<rpc::Response> for musdk_common::Response<'static> {
    type Error = anyhow::Error;

    fn try_from(response: rpc::Response) -> Result<Self, Self::Error> {
        let status_code: u16 = response.status.try_into().context("status_code")?;

        if status_code < 100 && status_code > 600 {
            bail!("{} is out of range for HTTP response status", status_code)
        }

        Ok(Self {
            status: Status::new(status_code),
            headers: response
                .headers
                .into_iter()
                .map(header_from_proto)
                .collect(),
            body: Cow::Owned(response.body),
        })
    }
}
