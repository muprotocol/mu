// We must use a BTreeMap to ensure key ordering stays consistent.
use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug)]
pub struct Stack {
    pub name: String,
    pub version: String,
    pub services: Vec<Service>,
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(tag = "type")]
pub enum Service {
    Database(Database),
    Gateway(Gateway),
    Function(Function),
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Database {
    pub name: String,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Gateway {
    pub name: String,
    pub endpoints: BTreeMap<String, Vec<GatewayEndpoint>>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct GatewayEndpoint {
    pub method: HttpMethod,
    pub route_to: String,
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "snake_case")]
pub enum HttpMethod {
    Get,
    Head,
    Post,
    Put,
    Patch,
    Delete,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Function {
    pub name: String,
    pub binary: String,
    pub runtime: FunctionRuntime,
    pub env: BTreeMap<String, String>,
}

#[derive(Serialize, Deserialize, Debug)]
pub enum FunctionRuntime {
    #[serde(rename = "wasi1.0")]
    Wasi1_0,
}
