use crate::protos::stack::*;
use anyhow::{anyhow, Result};
use protobuf::EnumOrUnknown;

impl From<super::Stack> for Stack {
    fn from(stack: super::Stack) -> Self {
        fn convert_http_method(method: super::HttpMethod) -> EnumOrUnknown<HttpMethod> {
            match method {
                super::HttpMethod::Get => EnumOrUnknown::new(HttpMethod::GET),
                super::HttpMethod::Post => EnumOrUnknown::new(HttpMethod::POST),
                super::HttpMethod::Patch => EnumOrUnknown::new(HttpMethod::PATCH),
                super::HttpMethod::Put => EnumOrUnknown::new(HttpMethod::PUT),
                super::HttpMethod::Delete => EnumOrUnknown::new(HttpMethod::DELETE),
                super::HttpMethod::Head => EnumOrUnknown::new(HttpMethod::HEAD),
                super::HttpMethod::Options => EnumOrUnknown::new(HttpMethod::OPTIONS),
            }
        }

        fn convert_function_runtime(
            runtime: super::AssemblyRuntime,
        ) -> EnumOrUnknown<FunctionRuntime> {
            match runtime {
                super::AssemblyRuntime::Wasi1_0 => EnumOrUnknown::new(FunctionRuntime::WASI1_0),
            }
        }

        Stack {
            name: stack.name,
            version: stack.version,
            services: stack
                .services
                .into_iter()
                .map(|s| match s {
                    super::Service::KeyValueTable(t) => Service {
                        service: Some(service::Service::KeyValueTable(KeyValueTable {
                            name: t.name,
                            delete: matches!(t.delete, Some(true)),
                            ..Default::default()
                        })),
                        ..Default::default()
                    },
                    super::Service::StorageName(s) => Service {
                        service: Some(service::Service::StorageName(StorageName {
                            name: s.name,
                            delete: matches!(s.delete, Some(true)),
                            ..Default::default()
                        })),
                        ..Default::default()
                    },
                    super::Service::Gateway(g) => Service {
                        service: Some(service::Service::Gateway(Gateway {
                            name: g.name,
                            endpoints: g
                                .endpoints
                                .into_iter()
                                .map(|(path, eps)| GatewayEndpoints {
                                    path,
                                    endpoints: eps
                                        .into_iter()
                                        .map(|ep| GatewayEndpoint {
                                            method: convert_http_method(ep.method),
                                            route_to_assembly: ep.route_to.assembly,
                                            route_to_function: ep.route_to.function,
                                            ..Default::default()
                                        })
                                        .collect(),
                                    ..Default::default()
                                })
                                .collect(),
                            ..Default::default()
                        })),
                        ..Default::default()
                    },
                    super::Service::Function(f) => Service {
                        service: Some(service::Service::Function(Function {
                            name: f.name,
                            binary: f.binary,
                            env: f
                                .env
                                .into_iter()
                                .map(|(name, value)| EnvVar {
                                    name,
                                    value,
                                    ..Default::default()
                                })
                                .collect(),
                            runtime: convert_function_runtime(f.runtime),
                            memoryLimit: f.memory_limit.get_bytes(),
                            ..Default::default()
                        })),
                        ..Default::default()
                    },
                })
                .collect(),
            ..Default::default()
        }
    }
}

impl TryFrom<Stack> for super::Stack {
    type Error = anyhow::Error;

    fn try_from(stack: Stack) -> Result<Self> {
        fn convert_http_method(method: EnumOrUnknown<HttpMethod>) -> Result<super::HttpMethod> {
            method
                .enum_value()
                .map(|e| match e {
                    HttpMethod::GET => super::HttpMethod::Get,
                    HttpMethod::POST => super::HttpMethod::Post,
                    HttpMethod::PATCH => super::HttpMethod::Patch,
                    HttpMethod::PUT => super::HttpMethod::Put,
                    HttpMethod::DELETE => super::HttpMethod::Delete,
                    HttpMethod::HEAD => super::HttpMethod::Head,
                    HttpMethod::OPTIONS => super::HttpMethod::Options,
                })
                .map_err(|i| anyhow!("Unknown enum value {i} for type HttpMethod"))
        }

        fn convert_function_runtime(
            runtime: EnumOrUnknown<FunctionRuntime>,
        ) -> Result<super::AssemblyRuntime> {
            runtime
                .enum_value()
                .map(|r| match r {
                    FunctionRuntime::WASI1_0 => super::AssemblyRuntime::Wasi1_0,
                })
                .map_err(|i| anyhow!("Unknown enum value {i} for type FunctionRuntime"))
        }

        Ok(super::Stack {
            name: stack.name,
            version: stack.version,
            services: stack
                .services
                .into_iter()
                .map(|s| match s.service {
                    None => Err(anyhow!("Blank service encountered")),

                    Some(service::Service::KeyValueTable(d)) => {
                        Ok(super::Service::KeyValueTable(super::NameAndDelete {
                            name: d.name,
                            delete: Some(d.delete),
                        }))
                    }

                    Some(service::Service::StorageName(s)) => {
                        Ok(super::Service::StorageName(super::NameAndDelete {
                            name: s.name,
                            delete: Some(s.delete),
                        }))
                    }

                    Some(service::Service::Gateway(g)) => {
                        Ok(super::Service::Gateway(super::Gateway {
                            name: g.name,
                            endpoints: g
                                .endpoints
                                .into_iter()
                                .map(|eps| {
                                    anyhow::Ok((
                                        eps.path,
                                        eps.endpoints
                                            .into_iter()
                                            .map(|ep| {
                                                anyhow::Ok(super::GatewayEndpoint {
                                                    method: convert_http_method(ep.method)?,
                                                    route_to: crate::AssemblyAndFunction {
                                                        assembly: ep.route_to_assembly,
                                                        function: ep.route_to_function,
                                                    },
                                                })
                                            })
                                            .collect::<Result<Vec<_>, _>>()?,
                                    ))
                                })
                                .collect::<Result<super::HashMap<_, _>, _>>()?,
                        }))
                    }

                    Some(service::Service::Function(f)) => {
                        Ok(super::Service::Function(super::Function {
                            name: f.name,
                            binary: f.binary,
                            env: f.env.into_iter().map(|env| (env.name, env.value)).collect(),
                            runtime: convert_function_runtime(f.runtime)?,
                            memory_limit: byte_unit::Byte::from_bytes(f.memoryLimit),
                        }))
                    }
                })
                .collect::<Result<Vec<_>, _>>()?,
        })
    }
}
