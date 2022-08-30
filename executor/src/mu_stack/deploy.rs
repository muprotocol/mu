use std::path::Path;

use db::MuDB;
use thiserror::Error;
use tokio::task::spawn_blocking;

use crate::{
    gateway::GatewayManager,
    mudb::{client::DatabaseID, db},
    runtime::{
        types::{FunctionDefinition, FunctionID},
        Runtime,
    },
};

use super::{HttpMethod, Stack, StackID};

#[derive(Error, Debug)]
pub enum StackValidationError {
    #[error("Duplicate function name '{0}'")]
    DuplicateFunctionName(String),

    #[error("Duplicate database name '{0}'")]
    DuplicateDatabaseName(String),

    #[error("Duplicate gateway name '{0}'")]
    DuplicateGatewayName(String),

    #[error("Failed to fetch binary for function '{0}' due to {1}")]
    CannotFetchFunction(String, anyhow::Error),

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

    #[error("Failed to deploy functions due to: {0}")]
    FailedToDeployFunctions(anyhow::Error),

    #[error("Failed to deploy gateways due to: {0}")]
    FailedToDeployGateways(anyhow::Error),

    #[error("Failed to deploy databases due to: {0}")]
    FailedToDeployDatabases(anyhow::Error),
}

pub async fn deploy(
    id: StackID,
    stack: Stack,
    mut runtime: Box<dyn Runtime>,
    gateway_manager: Box<dyn GatewayManager>,
) -> Result<(), StackDeploymentError> {
    let stack = validate(stack).map_err(StackDeploymentError::ValidationError)?;

    // TODO: handle partial deployments

    // Step 1: Functions
    // Since functions need to be fetched from remote sources, they're more error-prone, so deploy them first
    let mut function_names = vec![];
    let mut function_defs = vec![];
    for func in stack.functions() {
        let function_source = fetch_function(&func.binary).await;
        function_defs.push(FunctionDefinition {
            id: FunctionID {
                stack_id: id,
                function_name: func.name.clone(),
            },
            source: function_source,
            runtime: func.runtime,
            envs: func.env.clone(),
        });
        function_names.push(&func.name);
    }
    runtime
        .add_functions(function_defs)
        .await
        .map_err(StackDeploymentError::FailedToDeployFunctions)?;

    // Step 2: Databases
    let id_copy = id.clone();
    let databases = stack.databases().map(Clone::clone).collect::<Vec<_>>();
    let db_names = spawn_blocking(move || {
        let mut db_names = vec![];
        for db in databases {
            let db_name = DatabaseID::database_name(id_copy, &db.name);
            if !MuDB::db_exists(&db_name)
                .map_err(|e| StackDeploymentError::FailedToDeployDatabases(e.into()))?
            {
                MuDB::create_db_with_default_config(db_name.clone())
                    .map_err(|e| StackDeploymentError::FailedToDeployDatabases(e.into()))?;
            }
            db_names.push(db_name);
        }
        Ok(db_names)
    })
    .await
    .map_err(|e| StackDeploymentError::FailedToDeployDatabases(e.into()))??;

    // Step 3: Gateways
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

    // Now that everything deployed successfully, remove all obsolete services

    let existing_gateways = gateway_manager
        .get_deployed_gateway_names(id)
        .await
        .unwrap_or(Some(vec![]))
        .unwrap_or(vec![]);
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

    let prefix = format!("{id}_");
    spawn_blocking(move || {
        for db_name in MuDB::query_databases_by_prefix(&prefix).unwrap_or(vec![]) {
            if !db_names.contains(&db_name) {
                MuDB::delete_db(&db_name).unwrap_or(());
            }
        }
    })
    .await
    .unwrap_or(());

    let existing_function_names = runtime.get_function_names(id).await.unwrap_or(vec![]);
    let mut functions_to_delete = vec![];
    for existing in existing_function_names {
        if !function_names.iter().any(|n| ***n == *existing) {
            functions_to_delete.push(existing);
        }
    }
    if functions_to_delete.len() > 0 {
        runtime
            .remove_functions(id, functions_to_delete)
            .await
            .unwrap_or(());
    }

    Ok(())
}

fn validate(stack: Stack) -> Result<Stack, StackValidationError> {
    // TODO
    Ok(stack)
}

async fn fetch_function(url: &String) -> Vec<u8> {
    // TODO
    std::fs::read(Path::new("./prototype/functions").join(url)).unwrap()
}

/*
download function

    pub async fn add(&mut self, stack_id: StackID, function: Function) -> Result<()> {
        let id = FunctionID {
            stack_id,
            function_name: function.name.clone(),
        };

        let source = Self::download_function_module(&function.binary).await?;
        let definition =
            FunctionDefinition::new(id.clone(), source, function.runtime, function.env);
        self.functions.insert(id, definition);
        Ok(())
    }

*/
