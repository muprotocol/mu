use std::{
    net::{IpAddr, Ipv4Addr},
    path::PathBuf,
};

use anyhow::{Context, Result};

use mu_gateway::{GatewayManager, GatewayManagerConfig};
use mu_runtime::{AssemblyDefinition, Runtime, RuntimeConfig};
use mu_stack::{AssemblyID, FunctionID, Gateway, StackID};
use musdk_common::{Request, Response};

use super::{database::Database, StackWithID};

pub const CACHE_PATH: &'static str = "target/runtime-cache";

pub async fn start(
    stack: StackWithID,
) -> Result<(
    Box<dyn Runtime>,
    Box<dyn GatewayManager>,
    Database,
    Vec<Gateway>,
    StackID,
)> {
    let (stack, stack_id) = stack;

    let runtime_config = RuntimeConfig {
        cache_path: PathBuf::from(CACHE_PATH),
        include_function_logs: true,
    };

    let database = Database::start().await?;

    //TODO: Report usage using the notifications
    let (runtime, _) = mu_runtime::start(database.db_manager.clone(), runtime_config).await?;

    let mut function_defs = vec![];

    for func in stack.functions() {
        let assembly_source = tokio::fs::read(&func.binary)
            .await
            .context("Failed to get function source")?;

        function_defs.push(AssemblyDefinition::try_new(
            AssemblyID {
                stack_id,
                assembly_name: func.name.clone(),
            },
            assembly_source.into(),
            func.runtime,
            func.env.clone(),
            func.memory_limit,
        ));
    }

    let function_defs = function_defs
        .into_iter()
        .collect::<Result<Vec<AssemblyDefinition>, mu_runtime::Error>>()?;

    runtime.add_functions(function_defs).await?;

    let gateway_config = GatewayManagerConfig {
        listen_address: IpAddr::V4(Ipv4Addr::LOCALHOST),
        listen_port: 12012,
    };

    //TODO: Report usage using the notifications
    let (gateway, _) = mu_gateway::start(gateway_config, {
        let runtime = runtime.clone();
        move |f, r| Box::pin(handle_request(f, r, runtime.clone()))
    })
    .await?;

    gateway
        .deploy_gateways(stack_id, stack.gateways().map(ToOwned::to_owned).collect())
        .await?;

    let db_client = database
        .db_manager
        .clone()
        .make_client()
        .await
        .context("couldn't create database client")?;

    let mut tables = vec![];
    for x in stack.databases() {
        let table_name = x
            .name
            .to_owned()
            .try_into()
            .context("couldn't create table_name")?;
        tables.push(table_name);
    }

    db_client
        .update_stack_tables(stack_id, tables)
        .await
        .context("failed to setup database")?;

    let gateways = stack.gateways().map(ToOwned::to_owned).collect();

    Ok((runtime, gateway, database, gateways, stack_id))
}

async fn handle_request(
    function_id: FunctionID,
    request: Request<'_>,
    runtime: Box<dyn Runtime>,
) -> Result<Response<'static>> {
    runtime
        .invoke_function(function_id, request)
        .await
        .map_err(Into::into)
}