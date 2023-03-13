use std::{
    net::{IpAddr, Ipv4Addr},
    path::PathBuf,
};

use anyhow::{Context, Result};

use db_embedded_tikv::DbManagerWithTikv;
use mu_db::DeleteTable;
use mu_gateway::{GatewayManager, GatewayManagerConfig};
use mu_runtime::{AssemblyDefinition, Runtime, RuntimeConfig};
use mu_stack::{AssemblyID, FunctionID, Gateway, StackID};
use mu_storage::StorageManager;
use musdk_common::{Request, Response};

use super::StackWithID;

pub const CACHE_SUBDIR: &str = ".mu/runtime-cache";

pub async fn start(
    stack: StackWithID,
    project_root: PathBuf,
) -> Result<(
    Box<dyn Runtime>,
    Box<dyn GatewayManager>,
    DbManagerWithTikv,
    Box<dyn StorageManager>,
    Vec<Gateway>,
    StackID,
)> {
    let (stack, stack_id) = stack;

    let mut cache_path = project_root.clone();
    cache_path.push(CACHE_SUBDIR);

    // TODO: print usages at end of each function call/session to let users
    // know how much resources they are consuming
    let runtime_config = RuntimeConfig {
        cache_path,
        include_function_logs: true,
        max_giga_instructions_per_call: None,
    };

    let db_manager = super::database::start(project_root).await?;

    let storage_manager = super::storage::start().await?;

    //TODO: Report usage using the notifications
    let (runtime, _) =
        mu_runtime::start(db_manager.clone(), storage_manager.clone(), runtime_config).await?;

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
    let (gateway, _) = mu_gateway::start_without_additional_services(gateway_config, {
        let runtime = runtime.clone();
        move |f, r| Box::pin(handle_request(f, r, runtime.clone()))
    })
    .await?;

    gateway
        .deploy_gateways(stack_id, stack.gateways().map(ToOwned::to_owned).collect())
        .await?;

    let db_client = db_manager
        .make_client()
        .await
        .context("couldn't create database client")?;

    let mut table_actions = vec![];
    for kvt in stack.key_value_tables() {
        let table_name = kvt
            .name
            .clone()
            .try_into()
            .context("Failed to deploy tables")?;
        let delete = DeleteTable(matches!(kvt.delete, Some(true)));
        table_actions.push((table_name, delete));
    }

    db_client
        .update_stack_tables(stack_id, table_actions)
        .await
        .context("Failed to deploy tables")?;

    let gateways = stack.gateways().map(ToOwned::to_owned).collect();

    Ok((
        runtime,
        gateway,
        db_manager,
        storage_manager,
        gateways,
        stack_id,
    ))
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
