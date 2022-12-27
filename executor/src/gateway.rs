#![allow(clippy::too_many_arguments)]

use std::{borrow::Cow, collections::HashMap, net::IpAddr, path::PathBuf, sync::Arc};

use anyhow::{bail, Context, Result};
use async_trait::async_trait;
use dyn_clonable::clonable;
use log::{debug, error, trace};
use mailbox_processor::{callback::CallbackMailboxProcessor, ReplyChannel, RequestReplyChannel};
use mu_stack::{Gateway, HttpMethod, StackID};
use musdk_common::{Header, Request, Response};
use rocket::{
    catch, catchers, delete, get, head,
    http::Status,
    options, patch, post, put,
    request::{FromParam, FromRequest},
    routes, State,
};
use serde::Deserialize;
use tokio::{
    sync::{mpsc, RwLock},
    task::JoinHandle,
};

use crate::{
    network::{connection_manager::ConnectionManager, rpc_handler::RpcHandler, NodeConnection},
    request_routing::RoutingTarget,
    runtime::{
        types::{AssemblyID, FunctionID},
        Runtime,
    },
    stack::usage_aggregator::{Usage, UsageAggregator},
};

#[async_trait]
#[clonable]
pub trait GatewayManager: Clone + Send + Sync {
    async fn get_deployed_gateway_names(&self, stack_id: StackID) -> Result<Option<Vec<String>>>;
    async fn deploy_gateways(&self, stack_id: StackID, gateways: Vec<Gateway>) -> Result<()>;
    async fn delete_gateways(&self, stack_id: StackID, gateways: Vec<String>) -> Result<()>;
    async fn stop(&self) -> Result<()>;
}

#[derive(Deserialize)]
pub struct GatewayManagerConfig {
    pub listen_address: IpAddr,
    pub listen_port: u16,
}

type GatewayName = String;
type RequestPath = String;
type AssemblyName = String;
type FunctionName = String;

enum GatewayMessage {
    GetAssemblyAndFunction(
        StackID,
        GatewayName,
        HttpMethod,
        RequestPath,
        ReplyChannel<Option<(AssemblyName, FunctionName)>>,
    ),
    GetDeployedGatewayNames(StackID, ReplyChannel<Option<Vec<GatewayName>>>),
    DeployGateways(StackID, Vec<Gateway>),
    DeleteGateways(StackID, Vec<GatewayName>),
    Stop(),
}

#[derive(Clone)]
struct GatewayManagerImpl {
    mailbox: CallbackMailboxProcessor<GatewayMessage>,
}

#[async_trait]
impl GatewayManager for GatewayManagerImpl {
    async fn get_deployed_gateway_names(&self, stack_id: StackID) -> Result<Option<Vec<String>>> {
        self.mailbox
            .post_and_reply(|r| GatewayMessage::GetDeployedGatewayNames(stack_id, r))
            .await
            .map_err(Into::into)
    }

    async fn deploy_gateways(&self, stack_id: StackID, gateways: Vec<Gateway>) -> Result<()> {
        self.mailbox
            .post(GatewayMessage::DeployGateways(stack_id, gateways))
            .await
            .map_err(Into::into)
    }

    async fn delete_gateways(&self, stack_id: StackID, gateways: Vec<String>) -> Result<()> {
        self.mailbox
            .post(GatewayMessage::DeleteGateways(stack_id, gateways))
            .await
            .map_err(Into::into)
    }

    async fn stop(&self) -> Result<()> {
        self.mailbox.post(GatewayMessage::Stop()).await?;
        self.mailbox.clone().stop().await;
        Ok(())
    }
}

struct GatewayState {
    shutdown: Option<rocket::Shutdown>,
    server_future: Option<JoinHandle<()>>,
    gateways: HashMap<StackID, HashMap<String, Gateway>>,
}

// Used to access the gateway manager from within request handlers
struct DependencyAccessor {
    // TODO: break gateway manager's function management logic into new type to avoid
    // dependency cycle and remove need for Arc<RwLock>
    gateway_manager: Arc<RwLock<Option<GatewayManagerImpl>>>,
    runtime: Box<dyn Runtime>,
    connection_manager: Box<dyn ConnectionManager>,
    rpc_handler: Box<dyn RpcHandler>,
    usage_aggregator: Box<dyn UsageAggregator>,

    // We can't take a reference to the scheduler here, because the
    // scheduler also needs a reference to the gateway manager to
    // deploy the gateways to it.
    // This problem could be worked around differently, by e.g.
    // having the scheduler report the stacks it wants to deploy
    // rather than deploy them itself, which is not a bad idea for
    // a refactor.
    // TODO ^^^
    // Also, another way of going about this is to make all the
    // different mailboxes and put them in a static variable for
    // everything else to access as they see fit, but it makes
    // dependency tracking near impossible.
    get_routing_target: RequestReplyChannel<StackID, Result<RoutingTarget>>,
}

impl Clone for DependencyAccessor {
    fn clone(&self) -> Self {
        Self {
            gateway_manager: self.gateway_manager.clone(),
            runtime: self.runtime.clone(),
            connection_manager: self.connection_manager.clone(),
            rpc_handler: self.rpc_handler.clone(),
            usage_aggregator: self.usage_aggregator.clone(),
            get_routing_target: self.get_routing_target.clone(),
        }
    }
}

impl<'a> DependencyAccessor {
    async fn get_gateway_manager(&'a self) -> GatewayManagerImpl {
        self.gateway_manager.read().await.as_ref().unwrap().clone()
    }
}

// TODO: route requests through outer layer to enable passing to other nodes
pub async fn start(
    config: GatewayManagerConfig,
    runtime: Box<dyn Runtime>,
    connection_manager: Box<dyn ConnectionManager>,
    rpc_handler: Box<dyn RpcHandler>,
    usage_aggregator: Box<dyn UsageAggregator>,
) -> Result<(
    Box<dyn GatewayManager>,
    mpsc::UnboundedReceiver<(StackID, ReplyChannel<Result<RoutingTarget>>)>,
)> {
    let config = rocket::Config::figment()
        .merge(("address", config.listen_address.to_string()))
        .merge(("port", config.listen_port))
        .merge(("cli-colors", false))
        .merge(("ctrlc", false));

    let (req_rep_channel, req_rep_receiver) = RequestReplyChannel::new();

    let accessor = DependencyAccessor {
        gateway_manager: Arc::new(RwLock::new(None)),
        runtime,
        connection_manager,
        get_routing_target: req_rep_channel,
        rpc_handler,
        usage_aggregator,
    };

    let ignited = rocket::custom(config)
        .mount("/", routes![get, head, post, put, delete, patch, options])
        .register("/", catchers![catch])
        .manage(accessor.clone()) // TODO: DI-like solution?
        .ignite()
        .await?;

    let shutdown = ignited.shutdown();

    let server_future = tokio::spawn(async move {
        let result = ignited.launch().await;
        if let Err(f) = result {
            // TODO: notify outer layer if this happens prematurely
            error!("Failed to run rocket server: {f:?}");
        }
    });

    let state = GatewayState {
        shutdown: Some(shutdown),
        server_future: Some(server_future),
        gateways: HashMap::new(),
    };

    let mailbox = CallbackMailboxProcessor::start(step, state, 10000);

    let result = GatewayManagerImpl { mailbox };

    *accessor.gateway_manager.write().await = Some(result.clone());

    Ok((Box::new(result), req_rep_receiver))
}

async fn step(
    _mailbox: CallbackMailboxProcessor<GatewayMessage>,
    msg: GatewayMessage,
    mut state: GatewayState,
) -> GatewayState {
    match msg {
        GatewayMessage::GetAssemblyAndFunction(
            stack_id,
            gateway_name,
            method,
            request_path,
            rep,
        ) => {
            rep.reply(state.gateways.get(&stack_id).and_then(|gateways| {
                gateways.get(&gateway_name).and_then(|gateway| {
                    gateway.endpoints.get(&request_path).and_then(|eps| {
                        eps.iter()
                            .find(|ep| ep.method == method)
                            .map(|ep| (ep.route_to.assembly.clone(), ep.route_to.function.clone()))
                    })
                })
            }));
            state
        }

        GatewayMessage::GetDeployedGatewayNames(stack_id, rep) => {
            rep.reply(
                state
                    .gateways
                    .get(&stack_id)
                    .map(|gateways| gateways.keys().cloned().collect()),
            );
            state
        }

        GatewayMessage::DeployGateways(stack_id, incoming_gateways) => {
            let gateways = state.gateways.entry(stack_id).or_insert_with(HashMap::new);

            for incoming in incoming_gateways {
                gateways.insert(incoming.name.clone(), incoming);
            }

            state
        }

        GatewayMessage::DeleteGateways(stack_id, gateway_names) => {
            if let Some(gateways) = state.gateways.get_mut(&stack_id) {
                for name in gateway_names {
                    gateways.remove(&name);
                }
            }
            state
        }

        GatewayMessage::Stop() => {
            if let Some(shutdown) = state.shutdown.take() {
                shutdown.notify();
            }
            if let Some(f) = state.server_future.take() {
                if let Err(f) = f.await {
                    error!("Rocket failed to run to completion: {f:?}");
                }
            }
            state
        }
    }
}

struct StackIDParam(StackID);

impl<'a> FromParam<'a> for StackIDParam {
    type Error = ();

    fn from_param(param: &'a str) -> Result<Self, Self::Error> {
        param.parse().map(StackIDParam)
    }
}

#[derive(Debug)]
struct RequestHeaders<'a>(Vec<rocket::http::Header<'a>>);

#[async_trait]
impl<'a> FromRequest<'a> for RequestHeaders<'a> {
    type Error = ();

    async fn from_request(
        request: &'a rocket::Request<'_>,
    ) -> rocket::request::Outcome<Self, Self::Error> {
        let headers = request.headers();
        let map = headers.iter().collect();
        rocket::request::Outcome::Success(Self(map))
    }
}

fn calculate_request_size(r: &Request) -> u64 {
    let mut size = r.path.as_bytes().len() as u64;
    size += r
        .query
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
    let mut size = r.content_type.as_bytes().len() as u64;
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
        Self(Response {
            status: Status::BadRequest.code,
            content_type: "text/plain".into(),
            headers: vec![],
            body: Cow::Owned(description.into()),
        })
    }

    // TODO: does returning a 404 cause error catchers to run too?
    fn not_found() -> Self {
        Self(Response {
            status: Status::BadRequest.code,
            content_type: "text/plain".into(),
            headers: vec![],
            body: Cow::Owned("not found".into()),
        })
    }

    fn internal_error(description: &str) -> Self {
        Self(Response {
            status: Status::InternalServerError.code,
            content_type: "text/plain".into(),
            headers: vec![],
            body: Cow::Owned(description.into()),
        })
    }
}

impl<'r, 'o: 'r> rocket::response::Responder<'r, 'o> for ResponseWrapper {
    fn respond_to(self, _: &'r rocket::Request<'_>) -> rocket::response::Result<'o> {
        let mut builder = rocket::Response::build();

        builder.status(Status::new(self.0.status));

        for header in self.0.headers {
            builder.header(rocket::http::Header::new(
                header.name.into_owned(),
                header.value.into_owned(),
            ));
        }

        builder.header(rocket::http::Header::new(
            "Content-Type",
            self.0.content_type,
        ));

        builder.sized_body(self.0.body.len(), std::io::Cursor::new(self.0.body));

        builder.ok()
    }
}

// TODO: this function could be in a better location, but currently,
// only the gateway will be routing requests for the foreseeable future.
// TODO: alternatively, we could go with the initial idea of a transparent
// proxy layer between the gateway and runtime, though I don't believe it's
// justified, given that again, this is the only place we'll be routing
// requests.
async fn route_request<'a>(
    function_id: FunctionID,
    request: Request<'a>,
    dependency_accessor: &State<DependencyAccessor>,
) -> Result<ResponseWrapper> {
    trace!("Request received for {function_id}, will check deployment status");

    let route = dependency_accessor
        .get_routing_target
        .request(function_id.assembly_id.stack_id)
        .await
        .context("Failed to request route")?
        .context("Failed to find route")?;

    debug!(
        "Deployment status of stack {} is {:?}",
        function_id.assembly_id.stack_id, route
    );

    match route {
        RoutingTarget::NotDeployed => bail!("Stack not deployed"),
        RoutingTarget::Local => dependency_accessor
            .runtime
            .invoke_function(function_id, request)
            .await
            .map(ResponseWrapper)
            .map_err(Into::into),
        RoutingTarget::Remote(node_connection) => {
            let (connection_id, new_connection) = match node_connection {
                NodeConnection::Established(id) => (id, false),
                NodeConnection::NotEstablished(address) => {
                    // TODO! Does connecting to the target node here cause the gossip module to expect heartbeats?

                    // TODO should pool these connections so we don't do a connection handshake
                    // for each user request. QUIC is faster only if you're using an already open
                    // connection.
                    trace!("No connection to target node, will establish new connection");
                    let connection_id = dependency_accessor
                        .connection_manager
                        .connect(address.address, address.port)
                        .await
                        .context("Failed to connect to invocation target node")?;

                    (connection_id, true)
                }
            };

            trace!("Sending request");
            let response = dependency_accessor
                .rpc_handler
                .send_execute_function(connection_id, function_id, request)
                .await
                .context("Error in remote function invocation");
            trace!("Response received");

            if new_connection {
                trace!("Will disconnect new connection");
                // Nothing to do if disconnecting errors out
                let _ = dependency_accessor
                    .connection_manager
                    .disconnect(connection_id)
                    .await;
            }

            response.map(ResponseWrapper)
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

async fn handle_request<'a>(
    stack_id: StackIDParam,
    gateway_name: &'a str,
    method: HttpMethod,
    path: PathBuf,
    query: HashMap<&'a str, &'a str>,
    headers: RequestHeaders<'a>,
    data: Option<&'a [u8]>,
    dependency_accessor: &State<DependencyAccessor>,
) -> ResponseWrapper {
    let stack_id = stack_id.0;

    let path = match path.to_str() {
        Some(x) => x,
        None => return ResponseWrapper::bad_request("Invalid UTF-8 in request path"),
    };

    let query = query
        .into_iter()
        .map(|(k, v)| (Cow::Borrowed(k), Cow::Borrowed(v)))
        .collect::<HashMap<_, _>>();

    let headers = headers
        .0
        .iter()
        .map(|h| Header {
            name: Cow::Borrowed(h.name.as_str()),
            value: Cow::Borrowed(h.value()),
        })
        .collect();

    let request = Request {
        method: stack_http_method_to_sdk(method),
        path: Cow::Borrowed(path),
        query,
        headers,
        body: data.map(Cow::Borrowed).unwrap_or(Cow::Owned(vec![])),
    };

    let mut traffic = calculate_request_size(&request);

    let assembly_and_function = dependency_accessor
        .get_gateway_manager()
        .await
        .mailbox
        .post_and_reply(|r| {
            GatewayMessage::GetAssemblyAndFunction(
                stack_id,
                gateway_name.into(),
                method,
                (*request.path).to_owned(),
                r,
            )
        })
        .await;

    let (assembly_name, function_name) = match assembly_and_function {
        Err(_) => return ResponseWrapper::internal_error("Node is shutting down"),
        Ok(None) => return ResponseWrapper::not_found(),
        Ok(Some(x)) => x,
    };

    let response = match route_request(
        FunctionID {
            assembly_id: AssemblyID {
                stack_id,
                assembly_name,
            },
            function_name,
        },
        request,
        dependency_accessor,
    )
    .await
    {
        Ok(r) => {
            traffic += calculate_response_size(&r.0);
            r
        }
        // TODO: Generate meaningful error messages (propagate user function failure?)
        Err(f) => {
            error!("Failed to run user function: {f:?}");
            ResponseWrapper::internal_error("User function failure")
        }
    };

    dependency_accessor.usage_aggregator.register_usage(
        stack_id,
        vec![
            Usage::GatewayRequests { count: 1 },
            Usage::GatewayTraffic {
                size_bytes: traffic,
            },
        ],
    );

    response
}

#[get("/<stack_id>/<gateway_name>/<path..>?<query..>")]
async fn get<'a>(
    stack_id: StackIDParam,
    gateway_name: &'a str,
    path: PathBuf,
    query: HashMap<&'a str, &'a str>,
    headers: RequestHeaders<'a>,
    dependency_accessor: &State<DependencyAccessor>,
) -> ResponseWrapper {
    handle_request(
        stack_id,
        gateway_name,
        HttpMethod::Get,
        path,
        query,
        headers,
        None,
        dependency_accessor,
    )
    .await
}

#[post("/<stack_id>/<gateway_name>/<path..>?<query..>", data = "<data>")]
async fn post<'a>(
    stack_id: StackIDParam,
    gateway_name: &'a str,
    path: PathBuf,
    query: HashMap<&'a str, &'a str>,
    headers: RequestHeaders<'a>,
    data: &'a [u8],
    dependency_accessor: &State<DependencyAccessor>,
) -> ResponseWrapper {
    handle_request(
        stack_id,
        gateway_name,
        HttpMethod::Post,
        path,
        query,
        headers,
        Some(data),
        dependency_accessor,
    )
    .await
}

#[put("/<stack_id>/<gateway_name>/<path..>?<query..>", data = "<data>")]
async fn put<'a>(
    stack_id: StackIDParam,
    gateway_name: &'a str,
    path: PathBuf,
    query: HashMap<&'a str, &'a str>,
    headers: RequestHeaders<'a>,
    data: &'a [u8],
    dependency_accessor: &State<DependencyAccessor>,
) -> ResponseWrapper {
    handle_request(
        stack_id,
        gateway_name,
        HttpMethod::Put,
        path,
        query,
        headers,
        Some(data),
        dependency_accessor,
    )
    .await
}

#[delete("/<stack_id>/<gateway_name>/<path..>?<query..>", data = "<data>")]
async fn delete<'a>(
    stack_id: StackIDParam,
    gateway_name: &'a str,
    path: PathBuf,
    query: HashMap<&'a str, &'a str>,
    headers: RequestHeaders<'a>,
    data: &'a [u8],
    dependency_accessor: &State<DependencyAccessor>,
) -> ResponseWrapper {
    handle_request(
        stack_id,
        gateway_name,
        HttpMethod::Delete,
        path,
        query,
        headers,
        Some(data),
        dependency_accessor,
    )
    .await
}

#[head("/<stack_id>/<gateway_name>/<path..>?<query..>")]
async fn head<'a>(
    stack_id: StackIDParam,
    gateway_name: &'a str,
    path: PathBuf,
    query: HashMap<&'a str, &'a str>,
    headers: RequestHeaders<'a>,
    dependency_accessor: &State<DependencyAccessor>,
) -> ResponseWrapper {
    handle_request(
        stack_id,
        gateway_name,
        HttpMethod::Head,
        path,
        query,
        headers,
        None,
        dependency_accessor,
    )
    .await
}

#[patch("/<stack_id>/<gateway_name>/<path..>?<query..>", data = "<data>")]
async fn patch<'a>(
    stack_id: StackIDParam,
    gateway_name: &'a str,
    path: PathBuf,
    query: HashMap<&'a str, &'a str>,
    headers: RequestHeaders<'a>,
    data: &'a [u8],
    dependency_accessor: &State<DependencyAccessor>,
) -> ResponseWrapper {
    handle_request(
        stack_id,
        gateway_name,
        HttpMethod::Patch,
        path,
        query,
        headers,
        Some(data),
        dependency_accessor,
    )
    .await
}

#[options("/<stack_id>/<gateway_name>/<path..>?<query..>")]
async fn options<'a>(
    stack_id: StackIDParam,
    gateway_name: &'a str,
    path: PathBuf,
    query: HashMap<&'a str, &'a str>,
    headers: RequestHeaders<'a>,
    dependency_accessor: &State<DependencyAccessor>,
) -> ResponseWrapper {
    handle_request(
        stack_id,
        gateway_name,
        HttpMethod::Options,
        path,
        query,
        headers,
        None,
        dependency_accessor,
    )
    .await
}

#[catch(default)]
fn catch(status: Status, _request: &rocket::Request) -> String {
    match status.code {
        404 => "Not found".into(),
        _ => "".into(),
    }
}
