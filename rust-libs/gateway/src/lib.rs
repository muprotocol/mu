#![allow(clippy::too_many_arguments)]

use std::{borrow::Cow, collections::HashMap, future::Future, net::IpAddr, pin::Pin, sync::Arc};

use actix_web::{
    body::BoxBody,
    dev::ServerHandle,
    guard,
    http::{self, StatusCode},
    web, App, HttpRequest, HttpResponse, HttpServer, Resource, Responder,
};
use anyhow::{Context, Result};
use async_trait::async_trait;
use dyn_clonable::clonable;
use log::error;
use mailbox_processor::NotificationChannel;
use mu_stack::{AssemblyID, FunctionID, Gateway, StackID};
use musdk_common::{Header, Request, Response, Status};
use serde::Deserialize;
use tokio::sync::{mpsc, RwLock};

#[async_trait]
#[clonable]
pub trait GatewayManager: Clone + Send + Sync {
    async fn get_deployed_gateway_names(&self, stack_id: StackID) -> Result<Option<Vec<String>>>;
    async fn deploy_gateways(&self, stack_id: StackID, gateways: Vec<Gateway>) -> Result<()>;
    async fn delete_gateways(&self, stack_id: StackID, gateways: Vec<String>) -> Result<()>;
    async fn delete_all_gateways(&self, stack_id: StackID) -> Result<()>;
    async fn stop(&self) -> Result<()>;
}

//TODO: support multiple listen addresses, including Ipv6
#[derive(Deserialize)]
pub struct GatewayManagerConfig {
    pub listen_address: IpAddr,
    pub listen_port: u16,
}

#[derive(Clone)]
pub enum Notification {
    ReportUsage {
        stack_id: StackID,
        traffic: u64,
        requests: u64,
    },
}

type PathParams<'a> = HashMap<Cow<'a, str>, Cow<'a, str>>;
type Gateways = HashMap<StackID, HashMap<String, Gateway>>;

#[derive(Clone)]
struct GatewayManagerImpl {
    server_handle: ServerHandle,
    gateways: Arc<RwLock<Gateways>>,
}

#[async_trait]
impl GatewayManager for GatewayManagerImpl {
    async fn get_deployed_gateway_names(&self, stack_id: StackID) -> Result<Option<Vec<String>>> {
        Ok(self
            .gateways
            .read()
            .await
            .get(&stack_id)
            .map(|gateways| gateways.keys().cloned().collect()))
    }

    async fn deploy_gateways(
        &self,
        stack_id: StackID,
        incoming_gateways: Vec<Gateway>,
    ) -> Result<()> {
        let mut gateways = self.gateways.write().await;
        let entry = gateways.entry(stack_id).or_insert_with(HashMap::new);

        for incoming in incoming_gateways {
            entry.insert(incoming.name.clone(), incoming);
        }
        Ok(())
    }

    async fn delete_gateways(&self, stack_id: StackID, gateway_names: Vec<String>) -> Result<()> {
        if let Some(gateways) = self.gateways.write().await.get_mut(&stack_id) {
            for name in gateway_names {
                gateways.remove(&name);
            }
        }
        Ok(())
    }

    async fn delete_all_gateways(&self, stack_id: StackID) -> Result<()> {
        self.gateways.write().await.remove(&stack_id);
        Ok(())
    }

    async fn stop(&self) -> Result<()> {
        self.server_handle.stop(true).await;
        Ok(())
    }
}

// Used to access the gateway manager from within request handlers
struct DependencyAccessor<F> {
    gateways: Arc<RwLock<Gateways>>,
    handle_request: F,
    notification_channel: NotificationChannel<Notification>,
}

impl<F> Clone for DependencyAccessor<F>
where
    F: Clone,
{
    fn clone(&self) -> Self {
        Self {
            gateways: self.gateways.clone(),
            handle_request: self.handle_request.clone(),
            notification_channel: self.notification_channel.clone(),
        }
    }
}

fn match_path_and_extract_path_params<'a, 'ep>(
    request_path: &'a str,
    endpoint_path: &'ep str,
) -> Option<PathParams<'a>> {
    //TODO: Cache `endpoint_path` path segments for future matches
    let mut request_path_segments = request_path.split('/');
    let mut endpoint_path_segments = endpoint_path.split('/');

    let mut path_params = HashMap::new();

    loop {
        match (request_path_segments.next(), endpoint_path_segments.next()) {
            (Some(req_segment), Some(ep_segment)) => {
                //TODO: Check for cases like `/get/{a}{b}/` which is invalid, since there
                //is two variables in one segment.

                if req_segment == ep_segment {
                    continue;
                } else if ep_segment.starts_with('{') && ep_segment.ends_with('}') {
                    path_params.insert(
                        Cow::Owned(ep_segment[1..ep_segment.len() - 1].to_string()),
                        Cow::Borrowed(req_segment),
                    );
                } else {
                    return None;
                }
            }

            (None, None) => return Some(path_params),
            (None, Some(_)) | (Some(_), None) => return None,
        }
    }
}

pub async fn start<F>(
    config: GatewayManagerConfig,
    handle_request_callback: F,
) -> Result<(
    Box<dyn GatewayManager>,
    mpsc::UnboundedReceiver<Notification>,
)>
where
    for<'a> F: (Fn(
            FunctionID,
            Request<'a>,
        ) -> Pin<Box<dyn Future<Output = Result<Response<'static>>> + Send + 'a>>)
        // TODO: we're using a box because I don't know how I can use 'a in two where
        // clauses, so I can't express the same lifetime bound with a generic future
        + Clone
        + Send
        + Sync
        + 'static,
{
    let (tx, rx) = NotificationChannel::<Notification>::new();

    let gateways = Arc::new(RwLock::new(HashMap::new()));

    let accessor: DependencyAccessor<F> = {
        let gateways = gateways.clone();
        DependencyAccessor {
            gateways,
            handle_request: handle_request_callback,
            notification_channel: tx,
        }
    };

    let server = HttpServer::new(move || {
        App::new()
            .app_data(web::Data::new(accessor.clone()))
            .service(
                Resource::new("/{stack_id}/{gateway_name}/{path:.*}")
                    .guard(
                        guard::Any(guard::Get())
                            .or(guard::Post())
                            .or(guard::Put())
                            .or(guard::Delete())
                            .or(guard::Head())
                            .or(guard::Options())
                            .or(guard::Patch()),
                    )
                    .to(handle_request::<F>),
            )
            .default_service(web::to(|| async { ResponseWrapper::not_found() }))
    })
    .bind((config.listen_address, config.listen_port))
    .context("Failed to bind HTTP server port")?
    .disable_signals()
    .shutdown_timeout(15 * 60)
    .run();

    let server_handle = server.handle();

    tokio::spawn(server);

    let gateway_manager_impl = GatewayManagerImpl {
        server_handle,
        gateways,
    };

    Ok((Box::new(gateway_manager_impl), rx))
}
fn calculate_request_size(r: &Request) -> u64 {
    //let mut size = r.path.as_bytes().len() as u64; //TODO: check if we can calculate this
    let mut size = r
        .query_params
        .iter()
        .map(|x| x.0.as_bytes().len() as u64 + x.1.as_bytes().len() as u64)
        .sum::<u64>();
    size += r
        .headers
        .iter()
        .map(|x| x.name.as_bytes().len() as u64 + x.value.as_bytes().len() as u64)
        .sum::<u64>();
    size += r.body.len() as u64;
    size
}

fn calculate_response_size(r: &Response) -> u64 {
    let mut size = 0;
    size += r
        .headers
        .iter()
        .map(|x| x.name.as_bytes().len() as u64 + x.value.as_bytes().len() as u64)
        .sum::<u64>();
    size += r.body.len() as u64;
    size
}

struct ResponseWrapper(Response<'static>);

impl ResponseWrapper {
    fn bad_request(description: &str) -> Self {
        Self(
            Response::builder()
                .status(Status::BadRequest)
                .body_from_string(description.to_string()),
        )
    }

    fn not_found() -> Self {
        Self(
            Response::builder()
                .status(Status::NotFound)
                .body_from_str(Status::NotFound.reason().unwrap()),
        )
    }

    fn internal_error(description: &str) -> Self {
        Self(
            Response::builder()
                .status(Status::InternalServerError)
                .body_from_string(description.to_string()),
        )
    }
}

impl Responder for ResponseWrapper {
    type Body = BoxBody;

    #[allow(clippy::only_used_in_recursion)] // not our choice to pass this param, it's in the trait
    fn respond_to(self, req: &HttpRequest) -> actix_web::HttpResponse<Self::Body> {
        let Ok(status) = StatusCode::from_u16(self.0.status.code) else {
            return Self::internal_error("Invalid status code received from user function").respond_to(req);
        };

        let mut builder = HttpResponse::build(status);

        for header in self.0.headers {
            builder.append_header((header.name.into_owned(), header.value.into_owned()));
        }

        if self.0.body.len() > 0 {
            builder.body(self.0.body.into_owned())
        } else {
            builder.finish()
        }
    }
}

fn stack_http_method_to_sdk(method: mu_stack::HttpMethod) -> musdk_common::HttpMethod {
    match method {
        mu_stack::HttpMethod::Get => musdk_common::HttpMethod::Get,
        mu_stack::HttpMethod::Put => musdk_common::HttpMethod::Put,
        mu_stack::HttpMethod::Post => musdk_common::HttpMethod::Post,
        mu_stack::HttpMethod::Delete => musdk_common::HttpMethod::Delete,
        mu_stack::HttpMethod::Options => musdk_common::HttpMethod::Options,
        mu_stack::HttpMethod::Patch => musdk_common::HttpMethod::Patch,
        mu_stack::HttpMethod::Head => musdk_common::HttpMethod::Head,
    }
}

fn actix_http_method_to_stack(method: &http::Method) -> mu_stack::HttpMethod {
    if http::Method::GET == method {
        mu_stack::HttpMethod::Get
    } else if http::Method::POST == method {
        mu_stack::HttpMethod::Post
    } else if http::Method::PUT == method {
        mu_stack::HttpMethod::Put
    } else if http::Method::DELETE == method {
        mu_stack::HttpMethod::Delete
    } else if http::Method::OPTIONS == method {
        mu_stack::HttpMethod::Options
    } else if http::Method::PATCH == method {
        mu_stack::HttpMethod::Patch
    } else if http::Method::HEAD == method {
        mu_stack::HttpMethod::Head
    } else {
        panic!("Unexpected HTTP method {}", method.as_str());
    }
}

async fn handle_request<F>(
    request: HttpRequest,
    payload: Option<web::Bytes>,
    dependency_accessor: web::Data<DependencyAccessor<F>>,
) -> ResponseWrapper
where
    for<'a> F: (Fn(
            FunctionID,
            Request<'a>,
        ) -> Pin<Box<dyn Future<Output = Result<Response<'static>>> + Send + 'a>>)
        + Clone
        + Send
        + Sync
        + 'static,
{
    let Ok(stack_id) = request.match_info().get("stack_id").unwrap().parse() else {
        return ResponseWrapper::not_found();
    };

    let gateway_name = request.match_info().get("gateway_name").unwrap();
    let request_path = request.match_info().get("path").unwrap();

    let method = actix_http_method_to_stack(request.method());

    let Ok(headers) = request
        .headers()
        .iter()
        .map(|(k, v)| Ok(Header{name: Cow::Borrowed(k.as_str()), value: Cow::Borrowed(v.to_str()?)}))
        .collect::<Result<Vec<_>>>() else {
            return ResponseWrapper::bad_request("Invalid header values in request");
        };

    let Ok(query_params) =
        web::Query::<HashMap<Cow<'_, str>, Cow<'_, str>>>::from_query(
            request.query_string()
        ) else {
            return ResponseWrapper::bad_request("Invalid query string");
        };
    let query_params = query_params.into_inner();

    let gateways = dependency_accessor.gateways.read().await;
    let Some(gateway) = gateways.get(&stack_id).and_then(|s| s.get(gateway_name)) else {
        return ResponseWrapper::not_found();
    };

    let path_match_result = gateway
        .endpoints
        .iter()
        .find_map(|(path, eps)| {
            match_path_and_extract_path_params(request_path, path)
                .map(|path_params| (path_params, eps))
        })
        .and_then(|(path_params, eps)| {
            eps.iter().find(|ep| ep.method == method).map(|ep| {
                (
                    ep.route_to.assembly.clone(),
                    ep.route_to.function.clone(),
                    path_params,
                )
            })
        });

    drop(gateways);

    let Some((assembly_name, function_name, path_params)) = path_match_result else {
        return ResponseWrapper::not_found();
    };

    let request = Request {
        method: stack_http_method_to_sdk(method),
        path_params,
        query_params,
        headers,
        body: Cow::Borrowed(payload.as_ref().map(AsRef::as_ref).unwrap_or(&[])),
    };

    let mut traffic = calculate_request_size(&request);

    let response = match (dependency_accessor.handle_request)(
        FunctionID {
            assembly_id: AssemblyID {
                stack_id,
                assembly_name,
            },
            function_name,
        },
        request,
    )
    .await
    {
        Ok(r) => {
            traffic += calculate_response_size(&r);
            ResponseWrapper(r)
        }
        // TODO: Generate meaningful error messages (propagate user function failure?)
        Err(f) => {
            error!("Failed to run user function: {f:?}");
            ResponseWrapper::internal_error("User function failure")
        }
    };

    dependency_accessor
        .notification_channel
        .send(Notification::ReportUsage {
            stack_id,
            traffic,
            requests: 1,
        });

    response
}

#[cfg(test)]
mod tests {
    use super::match_path_and_extract_path_params;
    use std::collections::HashMap;

    #[test]
    fn simple_request_path_will_match() {
        let request_path = "/get/users/";
        let endpoint_path = "/get/users/";

        assert_eq!(
            Some(HashMap::new()),
            match_path_and_extract_path_params(request_path, endpoint_path)
        );
    }

    #[test]
    fn can_extract_single_path_param() {
        assert_eq!(
            Some([("id".into(), "12".into())].into()),
            match_path_and_extract_path_params("/get/user/12", "/get/user/{id}")
        );
    }

    #[test]
    fn can_extract_multi_path_param() {
        assert_eq!(
            Some([("type".into(), "user".into()), ("id".into(), "12".into())].into()),
            match_path_and_extract_path_params("/get/user/12", "/get/{type}/{id}")
        );
    }

    #[test]
    fn can_not_extract_path_params_from_empty_segments() {
        assert_eq!(
            None,
            match_path_and_extract_path_params("/get//12", "get/{type}/{id}/")
        );
    }

    #[test]
    fn incorrect_paths_wont_match() {
        assert_eq!(
            None,
            match_path_and_extract_path_params("/get/user/", "get/{type}/{id}/")
        );

        assert_eq!(
            None,
            match_path_and_extract_path_params("/get/user", "get/{type}/{id}/")
        );

        assert_eq!(
            None,
            match_path_and_extract_path_params("/get/", "get/{type}/{id}/")
        );

        assert_eq!(
            None,
            match_path_and_extract_path_params("/get///", "get/{type}/{id}/")
        );

        assert_eq!(
            None,
            match_path_and_extract_path_params("/", "get/{type}/{id}/")
        );
    }

    #[test]
    fn paths_with_more_segments_wont_match() {
        assert_eq!(
            None,
            match_path_and_extract_path_params("/get/user/12/45", "get/{type}/{id}/")
        );
    }
}
