use mu_storage::{DeleteStorage, StorageManager};
use reqwest::Url;
use thiserror::Error;

use mu_db::{DbManager, DeleteTable};
use mu_gateway::GatewayManager;
use mu_runtime::{AssemblyDefinition, Runtime};
use mu_stack::{AssemblyID, Stack, StackID, ValidatedStack};

use super::blockchain_monitor::StackRemovalMode;

#[derive(Error, Debug)]
pub enum StackDeploymentError {
    #[error("Bad assembly definition")]
    BadAssemblyDefinition,

    #[error("Failed to fetch binary for function '{0}' due to {1}")]
    CannotFetchFunction(String, anyhow::Error),

    #[error("Failed to deploy functions due to: {0}")]
    FailedToDeployFunctions(anyhow::Error),

    #[error("Failed to deploy gateways due to: {0}")]
    FailedToDeployGateways(anyhow::Error),

    #[error("Failed to deploy tables due to: {0}")]
    FailedToDeployTables(anyhow::Error),

    #[error("Failed to deploy storage names due to: {0}")]
    FailedToDeployStorageNames(anyhow::Error),

    #[error("Failed to connect to muDB: {0}")]
    FailedToConnectToDatabase(anyhow::Error),

    #[error("Failed to connect to muStorage: {0}")]
    FailedToConnectToStorage(anyhow::Error),
}

pub(super) async fn deploy(
    id: StackID,
    stack: ValidatedStack,
    runtime: &dyn Runtime,
    db_manager: &dyn DbManager,
    storage_manager: &dyn StorageManager,
) -> Result<(), StackDeploymentError> {
    let db_client = db_manager
        .make_client()
        .await
        .map_err(StackDeploymentError::FailedToConnectToDatabase)?;

    let storage_client = storage_manager
        .make_client()
        .map_err(StackDeploymentError::FailedToConnectToStorage)?;

    // TODO: handle partial deployments

    // Step 1: Functions
    // Since functions need to be fetched from remote sources, they're more error-prone, so deploy them first
    let mut function_names = vec![];
    let mut function_defs = vec![];
    for func in stack.functions() {
        let binary_url = Url::parse(&func.binary)
            .map_err(|e| StackDeploymentError::CannotFetchFunction(func.name.clone(), e.into()))?;

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

    // Step 2: Database tables
    let table_delete_paris = stack
        .key_value_tables()
        .into_iter()
        .map(|kvt| {
            let table_name = kvt
                .name
                .clone()
                .try_into()
                .map_err(StackDeploymentError::FailedToDeployTables)?;
            let delete = DeleteTable(matches!(kvt.delete, Some(true)));
            Ok((table_name, delete))
        })
        .collect::<anyhow::Result<_, _>>()?;

    db_client
        .update_stack_tables(id, table_delete_paris)
        .await
        .map_err(|e| StackDeploymentError::FailedToDeployTables(e.into()))?;

    // Step 3: Storage names
    let storage_delete_pairs = stack
        .storages()
        .into_iter()
        .map(|n| {
            let name = n.name.as_str();
            let del = DeleteStorage(matches!(n.delete, Some(true)));
            (name, del)
        })
        .collect();

    storage_client
        .update_stack_storages(mu_storage::Owner::Stack(id), storage_delete_pairs)
        .await
        .map_err(StackDeploymentError::FailedToDeployStorageNames)?;

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
    storage_manager: &dyn StorageManager,
) -> anyhow::Result<()> {
    if let StackRemovalMode::Permanent = mode {
        delete_user_data_permanently(db_manager, storage_manager, id).await?;
    }

    runtime.remove_all_functions(id).await?;

    Ok(())
}

async fn delete_user_data_permanently(
    db_manager: &dyn DbManager,
    storage_manager: &dyn StorageManager,
    stack_id: StackID,
) -> anyhow::Result<()> {
    delete_user_data_permanently_from_database(db_manager, stack_id).await?;
    delete_user_data_permanently_from_storage(storage_manager, stack_id).await
}

async fn delete_user_data_permanently_from_database(
    db_manager: &dyn DbManager,
    stack_id: StackID,
) -> anyhow::Result<()> {
    let db_client = db_manager.make_client().await?;
    let table_names = db_client.table_list(stack_id, None).await?;

    for name in table_names.clone() {
        db_client.clear_table(stack_id, name).await?;
    }

    let table_delete_pairs = table_names
        .into_iter()
        .map(|name| (name, DeleteTable(true)))
        .collect();

    db_client
        .update_stack_tables(stack_id, table_delete_pairs)
        .await?;

    Ok(())
}

async fn delete_user_data_permanently_from_storage(
    storage_manager: &dyn StorageManager,
    stack_id: StackID,
) -> anyhow::Result<()> {
    let owner = mu_storage::Owner::Stack(stack_id);

    let storage_client = storage_manager.make_client()?;
    let storage_names = storage_client.storage_list(owner).await?;

    for name in storage_names.clone() {
        storage_client.remove_storage(owner, &name).await?;
    }

    let storage_and_deletes = storage_names
        .iter()
        .map(|name| (name.as_str(), DeleteStorage(true)))
        .collect();

    storage_client
        .update_stack_storages(owner, storage_and_deletes)
        .await?;

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
