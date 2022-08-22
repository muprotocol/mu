use anyhow::Result;
use async_trait::async_trait;
use mu::runtime::{
    function::{FunctionDefinition, FunctionID},
    FunctionProvider,
};
use std::collections::HashMap;

pub struct MapFunctionProvider {
    inner: HashMap<FunctionID, FunctionDefinition>,
}

impl MapFunctionProvider {
    pub fn new(map: HashMap<FunctionID, FunctionDefinition>) -> Self {
        Self { inner: map }
    }

    pub fn ids(&self) -> Vec<FunctionID> {
        self.inner.keys().map(ToOwned::to_owned).collect()
    }
}

#[async_trait]
impl FunctionProvider for MapFunctionProvider {
    async fn get(&mut self, id: &FunctionID) -> Result<&FunctionDefinition> {
        Ok(self.inner.get(&id).unwrap())
    }
}
