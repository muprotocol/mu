use mu_gateway::GatewayManager;
use mu_runtime::{AssemblyDefinition, Runtime};
use reqwest::Url;
use thiserror::Error;

use mu_db::{DbManager, DeleteTable};

use mu_stack::{AssemblyID, HttpMethod, Stack, StackID};

use super::blockchain_monitor::StackRemovalMode;

#[derive(Error, Debug)]
pub enum StackValidationError {
    #[error("Duplicate function name '{0}'")]
    DuplicateFunctionName(String),

    #[error("Duplicate key_value_table name '{0}'")]
    DuplicateKeyValueTableName(String),

    #[error("Duplicate gateway name '{0}'")]
    DuplicateGatewayName(String),

    #[error("Failed to fetch binary for function '{0}' due to {1}")]
    CannotFetchFunction(String, anyhow::Error),

    #[error("Invalid URL for function '{0}': {1}")]
    InvalidFunctionUrl(String, anyhow::Error),

    #[error("Unknown function name '{function}' in gateway '{gateway}'")]
    UnknownFunctionInGateway { function: String, gateway: String },

    #[error(
        "Duplicate endpoint with path '{path}' and method '{method:?}' in gateway '{gateway}'"
    )]
    DuplicateEndpointInGateway {
        gateway: String,
        path: String,
        method: HttpMethod,
    },
}

#[derive(Error, Debug)]
pub enum StackDeploymentError {
    #[error("Validation error: {0}")]
    ValidationError(StackValidationError),

    #[error("Bad assembly definition")]
    BadAssemblyDefinition,

    #[error("Failed to deploy functions due to: {0}")]
    FailedToDeployFunctions(anyhow::Error),

    #[error("Failed to deploy gateways due to: {0}")]
    FailedToDeployGateways(anyhow::Error),

    #[error("Failed to deploy key-value-tables due to: {0}")]
    FailedToDeployKeyValueTables(anyhow::Error),

    #[error("Failed to connect to muDB: {0}")]
    FailedToConnectToDatabase(anyhow::Error),
}

pub(super) async fn deploy(
    id: StackID,
    stack: Stack,
    runtime: &dyn Runtime,
    db: &dyn DbManager,
) -> Result<(), StackDeploymentError> {
    let stack = validate(stack).map_err(StackDeploymentError::ValidationError)?;

    let db_client = db
        .make_client()
        .await
        .map_err(StackDeploymentError::FailedToConnectToDatabase)?;

    // TODO: handle partial deployments

    // Step 1: Functions
    // Since functions need to be fetched from remote sources, they're more error-prone, so deploy them first
    let mut function_names = vec![];
    let mut function_defs = vec![];
    for func in stack.functions() {
        let binary_url = Url::parse(&func.binary).map_err(|e| {
            StackDeploymentError::ValidationError(StackValidationError::InvalidFunctionUrl(
                func.name.clone(),
                e.into(),
            ))
        })?;

        let function_source = download_function(binary_url)
            .await
            .map_err(|e| StackDeploymentError::FailedToDeployFunctions(e.into()))?;

        function_defs.push(
            AssemblyDefinition::try_new(
                AssemblyID {
                    stack_id: id,
                    assembly_name: func.name.clone(),
                },
                function_source,
                func.runtime,
                func.env.clone(),
                func.memory_limit,
            )
            .map_err(|_| StackDeploymentError::BadAssemblyDefinition)?,
        );
        function_names.push(&func.name);
    }
    runtime
        .add_functions(function_defs)
        .await
        .map_err(|e| StackDeploymentError::FailedToDeployFunctions(e.into()))?;

    // Step 2: Value-key-tables of Database
    let mut table_action_tuples = vec![];
    for x in stack.key_value_tables() {
        let table_name = x.name.to_owned().try_into().map_err(|e| {
            StackDeploymentError::FailedToDeployKeyValueTables(anyhow::anyhow!("{e}"))
        })?;
        let delete = DeleteTable(x.delete);
        table_action_tuples.push((table_name, delete));
    }
    db_client
        .update_stack_tables(id, table_action_tuples)
        .await
        .map_err(|e| StackDeploymentError::FailedToDeployKeyValueTables(anyhow::anyhow!("{e}")))?;

    let existing_function_names = runtime.get_function_names(id).await.unwrap_or_default();
    let mut functions_to_delete = vec![];
    for existing in existing_function_names {
        if !function_names.iter().any(|n| ***n == *existing) {
            functions_to_delete.push(existing);
        }
    }
    if !functions_to_delete.is_empty() {
        runtime
            .remove_functions(id, functions_to_delete)
            .await
            .unwrap_or(());
    }

    Ok(())
}

fn validate(stack: Stack) -> Result<Stack, StackValidationError> {
    // TODO - implement this in mu_stack, use it in CLI too
    Ok(stack)
}

async fn download_function(url: Url) -> Result<bytes::Bytes, StackDeploymentError> {
    // TODO: implement a better function storage scenario
    reqwest::get(url)
        .await
        .map_err(|e| StackDeploymentError::FailedToDeployFunctions(e.into()))?
        .bytes()
        .await
        .map_err(|e| StackDeploymentError::FailedToDeployFunctions(e.into()))
}

pub(super) async fn undeploy_stack(
    id: StackID,
    mode: StackRemovalMode,
    runtime: &dyn Runtime,
    db_manager: &dyn DbManager,
) -> anyhow::Result<()> {
    // TODO: have a policy for deleting user data from the database
    // It should handle deleted and suspended stacks differently

    if let StackRemovalMode::Permanent = mode {
        let db_client = db_manager.make_client().await?;
        let tables = db_client.table_list(id, None).await?;
        for table in tables.clone() {
            db_client.clear_table(id, table).await?;
        }
        let table_deletes = tables
            .into_iter()
            .map(|table| (table, DeleteTable(true)))
            .collect();
        db_client.update_stack_tables(id, table_deletes).await?;
    }

    runtime.remove_all_functions(id).await?;

    Ok(())
}

pub(super) async fn deploy_gateways(
    id: StackID,
    stack: &Stack,
    gateway_manager: &dyn GatewayManager,
) -> Result<(), StackDeploymentError> {
    let mut gateway_names = vec![];
    let mut gateways_to_deploy = vec![];
    for gw in stack.gateways() {
        gateways_to_deploy.push(gw.clone_normalized());
        gateway_names.push(&gw.name);
    }
    gateway_manager
        .deploy_gateways(id, gateways_to_deploy)
        .await
        .map_err(StackDeploymentError::FailedToDeployGateways)?;

    let existing_gateways = gateway_manager
        .get_deployed_gateway_names(id)
        .await
        .unwrap_or(Some(vec![]))
        .unwrap_or_default();
    let mut gateways_to_remove = vec![];
    for existing in existing_gateways {
        if !gateway_names.iter().any(|n| ***n == *existing) {
            gateways_to_remove.push(existing);
        }
    }
    gateway_manager
        .delete_gateways(id, gateways_to_remove)
        .await
        .unwrap_or(());

    Ok(())
}

pub(super) async fn undeploy_gateways(
    id: StackID,
    gateway_manager: &dyn GatewayManager,
) -> anyhow::Result<()> {
    gateway_manager.delete_all_gateways(id).await
}
