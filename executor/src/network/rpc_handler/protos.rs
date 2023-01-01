use std::borrow::Cow;

use anyhow::{anyhow, bail, Result};
use mu_stack::StackID;
use protobuf::{EnumOrUnknown, MessageField};

use crate::{gateway, runtime};

include!(concat!(env!("OUT_DIR"), "/protos/mod.rs"));

impl From<runtime::types::FunctionID> for rpc::FunctionID {
    fn from(id: runtime::types::FunctionID) -> Self {
        let StackID::SolanaPublicKey(pk) = id.stack_id;
        Self {
            stack_id: MessageField(Some(Box::new(rpc::StackID {
                id: Some(rpc::stack_id::Id::Solana(pk.into())),
                ..Default::default()
            }))),
            function_name: id.function_name,
            ..Default::default()
        }
    }
}

impl TryFrom<rpc::FunctionID> for runtime::types::FunctionID {
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
            stack_id,
            function_name: id.function_name,
        })
    }
}

fn header_to_proto(h: gateway::Header<'_>) -> rpc::Header {
    rpc::Header {
        name: h.name.into_owned(),
        value: h.value.into_owned(),
        ..Default::default()
    }
}

fn header_from_proto(h: rpc::Header) -> gateway::Header<'static> {
    gateway::Header {
        name: Cow::Owned(h.name),
        value: Cow::Owned(h.value),
    }
}

impl<'a> From<gateway::Request<'a>> for rpc::Request {
    fn from(request: gateway::Request<'a>) -> Self {
        // TODO: we have the same code in the mu_stack crate as well. We could
        // unify the two sources if we set up a really complex scenario in which
        // the proto files in this crate reference those of the mu_stack crate,
        // and then we'd have to make the codegen look in the other crate's code,
        // but it doesn't seem to want to do this. All in all, not worth it IMO.
        fn convert_http_method(method: mu_stack::HttpMethod) -> EnumOrUnknown<rpc::HttpMethod> {
            match method {
                mu_stack::HttpMethod::Get => EnumOrUnknown::new(rpc::HttpMethod::GET),
                mu_stack::HttpMethod::Post => EnumOrUnknown::new(rpc::HttpMethod::POST),
                mu_stack::HttpMethod::Patch => EnumOrUnknown::new(rpc::HttpMethod::PATCH),
                mu_stack::HttpMethod::Put => EnumOrUnknown::new(rpc::HttpMethod::PUT),
                mu_stack::HttpMethod::Delete => EnumOrUnknown::new(rpc::HttpMethod::DELETE),
                mu_stack::HttpMethod::Head => EnumOrUnknown::new(rpc::HttpMethod::HEAD),
                mu_stack::HttpMethod::Options => EnumOrUnknown::new(rpc::HttpMethod::OPTIONS),
            }
        }

        fn convert_query_param<'a>(q: (Cow<'a, str>, Cow<'a, str>)) -> rpc::QueryParam {
            let (k, v) = q;
            rpc::QueryParam {
                key: k.into_owned(),
                value: v.into_owned(),
                ..Default::default()
            }
        }

        Self {
            method: convert_http_method(request.method),
            path: request.path.into_owned(),
            query_params: request.query.into_iter().map(convert_query_param).collect(),
            headers: request.headers.into_iter().map(header_to_proto).collect(),
            body: request.data.into_owned(),
            ..Default::default()
        }
    }
}

impl TryFrom<rpc::Request> for gateway::Request<'static> {
    type Error = anyhow::Error;

    fn try_from(request: rpc::Request) -> Result<Self> {
        fn convert_http_method(
            method: EnumOrUnknown<rpc::HttpMethod>,
        ) -> Result<mu_stack::HttpMethod> {
            method
                .enum_value()
                .map(|e| match e {
                    rpc::HttpMethod::GET => mu_stack::HttpMethod::Get,
                    rpc::HttpMethod::POST => mu_stack::HttpMethod::Post,
                    rpc::HttpMethod::PATCH => mu_stack::HttpMethod::Patch,
                    rpc::HttpMethod::PUT => mu_stack::HttpMethod::Put,
                    rpc::HttpMethod::DELETE => mu_stack::HttpMethod::Delete,
                    rpc::HttpMethod::HEAD => mu_stack::HttpMethod::Head,
                    rpc::HttpMethod::OPTIONS => mu_stack::HttpMethod::Options,
                })
                .map_err(|i| anyhow!("Unknown enum value {i} for type HttpMethod"))
        }

        fn convert_query_param(q: rpc::QueryParam) -> (Cow<'static, str>, Cow<'static, str>) {
            (Cow::Owned(q.key), Cow::Owned(q.value))
        }

        fn header_from_proto(h: rpc::Header) -> gateway::Header<'static> {
            gateway::Header {
                name: Cow::Owned(h.name),
                value: Cow::Owned(h.value),
            }
        }

        Ok(Self {
            method: convert_http_method(request.method)?,
            path: Cow::Owned(request.path),
            query: request
                .query_params
                .into_iter()
                .map(convert_query_param)
                .collect(),
            headers: request.headers.into_iter().map(header_from_proto).collect(),
            data: Cow::Owned(request.body),
        })
    }
}

impl From<gateway::Response> for rpc::Response {
    fn from(response: gateway::Response) -> Self {
        Self {
            status: response.status as i32,
            content_type: response.content_type,
            headers: response.headers.into_iter().map(header_to_proto).collect(),
            body: response.body,
            ..Default::default()
        }
    }
}

impl TryFrom<rpc::Response> for gateway::Response {
    type Error = anyhow::Error;

    fn try_from(response: rpc::Response) -> Result<Self, Self::Error> {
        let status = if response.status >= 100 && response.status < 600 {
            response.status as u16
        } else {
            bail!(
                "{} is out of range for HTTP response status",
                response.status
            )
        };

        Ok(Self {
            status,
            content_type: response.content_type,
            headers: response
                .headers
                .into_iter()
                .map(header_from_proto)
                .collect(),
            body: response.body,
        })
    }
}
