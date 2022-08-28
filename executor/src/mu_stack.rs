use std::{collections::HashMap, fmt::Display};

use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Clone, Debug, Hash, PartialEq, Eq)]
pub struct StackID(pub Uuid);

impl Display for StackID {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

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
    pub endpoints: HashMap<String, Vec<GatewayEndpoint>>,
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
    pub env: HashMap<String, String>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub enum FunctionRuntime {
    #[serde(rename = "wasi1.0")]
    Wasi1_0,
}
