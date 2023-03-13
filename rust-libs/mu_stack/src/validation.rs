use std::{collections::HashSet, hash::Hash, ops::Deref};

use thiserror::Error;

use crate::{HttpMethod, Stack};

#[derive(Clone, Debug, Default)]
pub struct ValidatedStack(Stack);

impl ValidatedStack {
    pub fn into_inner(self) -> Stack {
        self.0
    }
}

impl Deref for ValidatedStack {
    type Target = Stack;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

#[derive(Error, Debug)]
pub enum StackValidationError {
    #[error("Duplicate function name '{0}'")]
    DuplicateFunctionName(String),

    #[error("Duplicate table name '{0}'")]
    DuplicateTableName(String),

    #[error("Duplicate gateway name '{0}'")]
    DuplicateGatewayName(String),

    #[error("Duplicate storage name '{0}'")]
    DuplicateStorageName(String),

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

macro_rules! attempt_with {
    ($ex:expr, $mk_err:expr, $stack:ident) => {
        match $ex {
            Ok(x) => x,
            Err(e) => {
                let e = $mk_err(e);
                return Err(($stack, e));
            }
        }
    };
}

#[allow(clippy::result_large_err)]
pub(super) fn validate(stack: Stack) -> Result<ValidatedStack, (Stack, StackValidationError)> {
    attempt_with!(
        ensure_all_unique(stack.functions().map(|f| &f.name)),
        |e: &String| StackValidationError::DuplicateFunctionName(e.clone()),
        stack
    );

    attempt_with!(
        ensure_all_unique(stack.key_value_tables().map(|t| &t.name)),
        |e: &String| StackValidationError::DuplicateTableName(e.clone()),
        stack
    );

    attempt_with!(
        ensure_all_unique(stack.gateways().map(|g| &g.name)),
        |e: &String| StackValidationError::DuplicateGatewayName(e.clone()),
        stack
    );

    attempt_with!(
        ensure_all_unique(stack.storages().map(|g| &g.name)),
        |e: &String| StackValidationError::DuplicateStorageName(e.clone()),
        stack
    );

    attempt_with!(ensure_gateway_functions_correct(&stack), |e| e, stack);

    let mut err = None;
    for gw in stack.gateways() {
        if let Err(e) = ensure_all_unique(
            gw.endpoints
                .iter()
                .flat_map(|(path, eps)| eps.iter().map(|ep| (path.clone(), ep.method))),
        ) {
            err = Some(StackValidationError::DuplicateEndpointInGateway {
                gateway: gw.name.clone(),
                path: e.0.clone(),
                method: e.1,
            });
        }
    }

    if let Some(err) = err {
        return Err((stack, err));
    }

    Ok(ValidatedStack(stack))
}

fn ensure_gateway_functions_correct(stack: &Stack) -> Result<(), StackValidationError> {
    for gw in stack.gateways() {
        for eps in gw.endpoints.values() {
            for ep in eps {
                if !stack.functions().any(|f| f.name == ep.route_to.assembly) {
                    return Err(StackValidationError::UnknownFunctionInGateway {
                        function: ep.route_to.assembly.clone(),
                        gateway: gw.name.clone(),
                    });
                }
            }
        }
    }
    Ok(())
}

fn ensure_all_unique<T: Hash + Eq + Clone>(it: impl Iterator<Item = T>) -> Result<(), T> {
    let mut hashset = HashSet::new();

    for i in it {
        if hashset.contains(&i) {
            return Err(i);
        }

        hashset.insert(i.clone());
    }

    Ok(())
}
