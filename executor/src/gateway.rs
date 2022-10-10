#![allow(clippy::too_many_arguments)]

use std::{borrow::Cow, collections::HashMap, net::IpAddr, path::PathBuf, sync::Arc};

use anyhow::Result;
use async_trait::async_trait;
use dyn_clonable::clonable;
use log::error;
use mailbox_processor::{callback::CallbackMailboxProcessor, ReplyChannel};
use mu_stack::{Gateway, HttpMethod, StackID};
use rocket::{
    catch, catchers, delete, get, head,
    http::Status,
    options, patch, post, put,
    request::{FromParam, FromRequest},
    routes, State,
};
use serde::{Deserialize, Serialize};
use tokio::{sync::RwLock, task::JoinHandle};

use crate::runtime::{types::FunctionID, Runtime};

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

enum GatewayMessage {
    GetFunctionName(
        StackID,
        GatewayName,
        HttpMethod,
        RequestPath,
        ReplyChannel<Option<String>>,
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
#[derive(Clone)]
struct DependencyAccessor {
    // TODO: break gateway manager's function management logic into new type to avoid
    // dependency cycle and remove need for Arc<RwLock>
    gateway_manager: Arc<RwLock<Option<GatewayManagerImpl>>>,
    runtime: Box<dyn Runtime>,
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
) -> Result<Box<dyn GatewayManager>> {
    let config = rocket::Config::figment()
        .merge(("address", config.listen_address.to_string()))
        .merge(("port", config.listen_port))
        .merge(("cli-colors", false))
        .merge(("ctrlc", false));

    let accessor = DependencyAccessor {
        gateway_manager: Arc::new(RwLock::new(None)),
        runtime,
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

    Ok(Box::new(result))
}

async fn step(
    _mailbox: CallbackMailboxProcessor<GatewayMessage>,
    msg: GatewayMessage,
    mut state: GatewayState,
) -> GatewayState {
    match msg {
        GatewayMessage::GetFunctionName(stack_id, gateway_name, method, request_path, rep) => {
            rep.reply(state.gateways.get(&stack_id).and_then(|gateways| {
                gateways.get(&gateway_name).and_then(|gateway| {
                    gateway.endpoints.get(&request_path).and_then(|eps| {
                        eps.iter()
                            .find(|ep| ep.method == method)
                            .map(|ep| ep.route_to.clone())
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

#[derive(Debug, Serialize)]
pub struct Header<'a> {
    pub name: Cow<'a, str>,
    pub value: Cow<'a, str>,
}

#[derive(Debug, Serialize)]
pub struct Request<'a> {
    pub method: HttpMethod,
    pub path: &'a str,
    pub query: HashMap<&'a str, &'a str>,
    pub headers: Vec<Header<'a>>,
    pub data: &'a str,
}

#[derive(Debug, Deserialize)]
pub struct OwnedHeader {
    pub name: String,
    pub value: String,
}

#[derive(Debug, Deserialize)]
pub struct Response {
    pub status: u16,
    pub content_type: String,
    pub headers: Vec<OwnedHeader>,
    pub body: String,
}

impl<'a> Response {
    fn bad_request(description: &'a str) -> Self {
        Self {
            status: Status::BadRequest.code,
            content_type: "text/plain".into(),
            headers: vec![],
            body: description.into(),
        }
    }

    // TODO: does returning a 404 cause error catchers to run too?
    fn not_found() -> Self {
        Self {
            status: Status::BadRequest.code,
            content_type: "text/plain".into(),
            headers: vec![],
            body: "not found".into(),
        }
    }

    fn internal_error(description: &'a str) -> Self {
        Self {
            status: Status::InternalServerError.code,
            content_type: "text/plain".into(),
            headers: vec![],
            body: description.into(),
        }
    }
}

impl<'r, 'o: 'r> rocket::response::Responder<'r, 'o> for Response {
    fn respond_to(self, _: &'r rocket::Request<'_>) -> rocket::response::Result<'o> {
        let mut builder = rocket::Response::build();

        builder.status(Status::new(self.status));

        for header in self.headers {
            builder.header(rocket::http::Header::new(
                header.name.to_owned(),
                header.value.to_owned(),
            ));
        }

        builder.header(rocket::http::Header::new("Content-Type", self.content_type));

        builder.sized_body(
            self.body.as_bytes().len(),
            std::io::Cursor::new(self.body.into_bytes()),
        );

        builder.ok()
    }
}

async fn handle_request<'a>(
    stack_id: StackIDParam,
    gateway_name: &'a str,
    method: HttpMethod,
    path: PathBuf,
    query: HashMap<&'a str, &'a str>,
    headers: RequestHeaders<'a>,
    data: Option<&'a str>,
    dependency_accessor: &State<DependencyAccessor>,
) -> Response {
    let stack_id = stack_id.0;

    let path = match path.to_str() {
        Some(x) => x,
        None => return Response::bad_request("Invalid UTF-8 in request path"),
    };

    let headers = headers
        .0
        .iter()
        .map(|h| Header {
            name: Cow::Borrowed(h.name.as_str()),
            value: Cow::Borrowed(h.value()),
        })
        .collect();

    let request = Request {
        method,
        path,
        query,
        headers,
        data: data.unwrap_or(""),
    };

    let function_name = dependency_accessor
        .get_gateway_manager()
        .await
        .mailbox
        .post_and_reply(|r| {
            GatewayMessage::GetFunctionName(
                stack_id,
                gateway_name.into(),
                request.method,
                request.path.into(),
                r,
            )
        })
        .await;

    let function_name = match function_name {
        Err(_) => return Response::internal_error("Node is shutting down"),
        Ok(None) => return Response::not_found(),
        Ok(Some(x)) => x,
    };

    match dependency_accessor
        .runtime
        .invoke_function(
            FunctionID {
                stack_id,
                function_name,
            },
            request,
        )
        .await
    {
        Ok(x) => x.0,
        // TODO: Generate meaningful error messages (propagate user function failure?)
        Err(f) => {
            error!("Failed to run user function: {f:?}");
            Response::internal_error("User function failure")
        }
    }
}

#[get("/<stack_id>/<gateway_name>/<path..>?<query..>", data = "<data>")]
async fn get<'a>(
    stack_id: StackIDParam,
    gateway_name: &'a str,
    path: PathBuf,
    query: HashMap<&'a str, &'a str>,
    headers: RequestHeaders<'a>,
    data: &'a str,
    dependency_accessor: &State<DependencyAccessor>,
) -> Response {
    handle_request(
        stack_id,
        gateway_name,
        HttpMethod::Get,
        path,
        query,
        headers,
        Some(data),
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
    data: &'a str,
    dependency_accessor: &State<DependencyAccessor>,
) -> Response {
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
    data: &'a str,
    dependency_accessor: &State<DependencyAccessor>,
) -> Response {
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
    data: &'a str,
    dependency_accessor: &State<DependencyAccessor>,
) -> Response {
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

#[head("/<stack_id>/<gateway_name>/<path..>?<query..>", data = "<data>")]
async fn head<'a>(
    stack_id: StackIDParam,
    gateway_name: &'a str,
    path: PathBuf,
    query: HashMap<&'a str, &'a str>,
    headers: RequestHeaders<'a>,
    data: &'a str,
    dependency_accessor: &State<DependencyAccessor>,
) -> Response {
    handle_request(
        stack_id,
        gateway_name,
        HttpMethod::Head,
        path,
        query,
        headers,
        Some(data),
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
    data: &'a str,
    dependency_accessor: &State<DependencyAccessor>,
) -> Response {
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

#[options("/<stack_id>/<gateway_name>/<path..>?<query..>", data = "<data>")]
async fn options<'a>(
    stack_id: StackIDParam,
    gateway_name: &'a str,
    path: PathBuf,
    query: HashMap<&'a str, &'a str>,
    headers: RequestHeaders<'a>,
    data: &'a str,
    dependency_accessor: &State<DependencyAccessor>,
) -> Response {
    handle_request(
        stack_id,
        gateway_name,
        HttpMethod::Options,
        path,
        query,
        headers,
        Some(data),
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
