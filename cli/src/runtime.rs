use std::{
    borrow::Cow,
    collections::HashMap,
    net::{IpAddr, Ipv4Addr},
    path::Path,
};

use anyhow::{bail, Context, Result};
use async_trait::async_trait;

use mu_gateway::{GatewayManager, GatewayManagerConfig};
use mu_runtime::{AssemblyDefinition, AssemblyProvider, Runtime, RuntimeConfig};
use mu_stack::{AssemblyID, FunctionID, Gateway, Stack, StackID};
use musdk_common::{Request, Response};

use crate::database::Database;

pub async fn start(
    stack: Stack,
    function_binary_path: &Path,
) -> Result<(
    Box<dyn Runtime>,
    Box<dyn GatewayManager>,
    Database,
    Vec<Gateway>,
    StackID,
)> {
    let stack_id = StackID::SolanaPublicKey(rand::random());

    let assembly_provider = MapAssemblyProvider::default();

    let runtime_config = RuntimeConfig {
        cache_path: Path::new("target/runtime-cache").to_path_buf(),
        include_function_logs: true,
    };

    let database = Database::start().await?;

    //TODO: Report usage using the notifications
    let (runtime, _) = mu_runtime::start(
        Box::new(assembly_provider),
        database.db_manager.clone(),
        runtime_config,
    )
    .await?;

    let mut function_defs = vec![];

    for func in stack.functions() {
        //TODO: We are not reading function url from stack.yaml file, because it's not uploaded to
        //the internet yet and can't download it.
        //  But should be able to download it or use it's path when we have other options in
        //stack.yaml

        let assembly_source = std::fs::read(function_binary_path)?;

        function_defs.push(AssemblyDefinition {
            id: AssemblyID {
                stack_id,
                assembly_name: func.name.clone(),
            },
            source: assembly_source.into(),
            runtime: func.runtime,
            envs: func.env.clone(),
            memory_limit: func.memory_limit,
        });
    }

    runtime.add_functions(function_defs.clone()).await?;

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

#[derive(Default)]
pub struct MapAssemblyProvider {
    inner: HashMap<AssemblyID, AssemblyDefinition>,
}

#[async_trait]
impl AssemblyProvider for MapAssemblyProvider {
    fn get(&self, id: &AssemblyID) -> Option<&AssemblyDefinition> {
        Some(self.inner.get(id).unwrap())
    }

    fn add_function(&mut self, assembly: AssemblyDefinition) {
        self.inner.insert(assembly.id.clone(), assembly);
    }

    fn remove_function(&mut self, id: &AssemblyID) {
        self.inner.remove(id);
    }

    fn get_function_names(&self, _stack_id: &StackID) -> Vec<String> {
        unimplemented!("Not needed")
    }

    fn remove_all_functions(&mut self, _stack_id: &StackID) -> Option<Vec<String>> {
        unimplemented!("Not needed")
    }
}

pub fn read_stack(project_directory: Option<&Path>) -> Result<Stack> {
    let path = match project_directory {
        Some(p) => Cow::Borrowed(p),
        None => Cow::Owned(std::env::current_dir()?),
    }
    .join("stack.yaml");

    if !path.try_exists()? {
        bail!("Not in a mu project, stack.yaml not found.");
    }

    let file = std::fs::File::open(path)?;
    serde_yaml::from_reader(file).map_err(Into::into)
}
