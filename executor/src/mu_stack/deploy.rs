use thiserror::Error;

use super::{Function, Gateway, HttpMethod, Stack, StackID};

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

pub async fn deploy(id: StackID, stack: Stack) -> Result<(), StackValidationError> {
    let stack = validate(stack)?;

    // TODO: handle partial deployments

    // Step 1: Functions
    // Since functions need to be fetched from remote sources, they're more error-prone, so deploy them first
    let mut function_names = vec![];
    for func in stack.functions() {
        let function_source = fetch_function(&func.binary).await;
        deploy_function(id, &func, function_source).await;
        function_names.push(&func.name);
    }

    // Step 2: Databases
    let mut db_names = vec![];
    for db in stack.databases() {
        let db_name = format!("{id}:{}", db.name);
        if !database_exists(&db_name).await {
            create_database(&db_name).await;
        }
        db_names.push(db_name);
    }

    // Step 3: Gateways
    let mut gateway_names = vec![];
    for gw in stack.gateways() {
        deploy_gateway(id, gw).await;
        gateway_names.push(&gw.name);
    }

    // Now that everything deployed successfully, remove all obsolete services

    for existing_gw in get_existing_gateways(id).await {
        if gateway_names
            .iter()
            .filter(|n| ***n == *existing_gw.name)
            .nth(0)
            == None
        {
            delete_gateway(id, existing_gw).await;
        }
    }

    let prefix = format!("{id}:");
    for db_name in query_databases_by_prefix(&prefix).await {
        if !db_names.contains(&db_name) {
            delete_database(&db_name).await;
        }
    }

    for existing_func in get_existing_functions(id).await {
        if function_names
            .iter()
            .filter(|n| ***n == *existing_func.name)
            .nth(0)
            == None
        {
            delete_function(id, existing_func).await;
        }
    }

    Ok(())
}

fn validate(stack: Stack) -> Result<Stack, StackValidationError> {
    // TODO
    Ok(stack)
}

// Stub implementations, to be filled in by implementations in each module

async fn database_exists(_db_name: &String) -> bool {
    true
}

async fn create_database(_db_name: &String) {}

async fn query_databases_by_prefix(_prefix: &String) -> Vec<String> {
    vec![]
}

async fn delete_database(_db_name: &String) {}

async fn fetch_function(_url: &String) -> Vec<u8> {
    vec![]
}

async fn deploy_function(_stack_id: StackID, _func: &Function, _source: Vec<u8>) {}

struct DeployedFunction {
    name: String,
}

async fn get_existing_functions(_stack_id: StackID) -> Vec<DeployedFunction> {
    vec![]
}

async fn delete_function(_stack_id: StackID, _function: DeployedFunction) {}

async fn deploy_gateway(_stack_id: StackID, _gw: &Gateway) {}

struct DeployedGateway {
    name: String,
}

async fn get_existing_gateways(_stack_id: StackID) -> Vec<DeployedGateway> {
    vec![]
}

async fn delete_gateway(_stack_id: StackID, _gateway: DeployedGateway) {}

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
