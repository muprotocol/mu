use std::borrow::Cow;

use anyhow::{anyhow, bail, Context, Result};
use mu_stack::StackID;
use musdk_common::Status;
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
            match method {
                musdk_common::HttpMethod::Get => EnumOrUnknown::new(rpc::HttpMethod::GET),
                musdk_common::HttpMethod::Post => EnumOrUnknown::new(rpc::HttpMethod::POST),
                musdk_common::HttpMethod::Patch => EnumOrUnknown::new(rpc::HttpMethod::PATCH),
                musdk_common::HttpMethod::Put => EnumOrUnknown::new(rpc::HttpMethod::PUT),
                musdk_common::HttpMethod::Delete => EnumOrUnknown::new(rpc::HttpMethod::DELETE),
                musdk_common::HttpMethod::Head => EnumOrUnknown::new(rpc::HttpMethod::HEAD),
                musdk_common::HttpMethod::Options => EnumOrUnknown::new(rpc::HttpMethod::OPTIONS),
            }
        }

        fn convert_key_value_pair<'a>(p: (Cow<'a, str>, Cow<'a, str>)) -> rpc::KeyValuePair {
            let (k, v) = p;
            rpc::KeyValuePair {
                key: k.into_owned(),
                value: v.into_owned(),
                ..Default::default()
            }
        }

        Self {
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

        Ok(Self {
            method: convert_http_method(request.method)?,
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
