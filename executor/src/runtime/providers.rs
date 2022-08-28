use super::{
    error::Error,
    types::{FunctionDefinition, FunctionID, FunctionProvider, FunctionSource},
};
use crate::mu_stack::{Function, StackID};
use anyhow::Result;
use async_trait::async_trait;
use std::collections::HashMap;

pub struct FunctionProviderImpl {
    functions: HashMap<FunctionID, FunctionDefinition>,
}

impl FunctionProviderImpl {
    pub fn new() -> Self {
        Self {
            functions: HashMap::new(),
        }
    }

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

    async fn download_function_module(url: &str) -> Result<FunctionSource> {
        let bytes = reqwest::get(url).await?.bytes().await?.to_vec();
        Ok(bytes)
    }
}

#[async_trait]
impl FunctionProvider for FunctionProviderImpl {
    async fn get(&mut self, id: &FunctionID) -> Result<&FunctionDefinition> {
        match self.functions.get(id) {
            Some(d) => Ok(d),
            None => Err(Error::FunctionNotFound(id.clone())).map_err(Into::into),
        }
    }
}
