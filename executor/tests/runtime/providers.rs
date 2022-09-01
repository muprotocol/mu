use async_trait::async_trait;
use mu::{mu_stack::StackID, runtime::types::*};
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
    fn get(&self, id: &FunctionID) -> Option<&FunctionDefinition> {
        Some(self.inner.get(id).unwrap())
    }

    fn add_function(&mut self, _function: FunctionDefinition) {
        unimplemented!("Not needed")
    }

    fn remove_function(&mut self, _id: &FunctionID) {
        unimplemented!("Not needed")
    }

    fn get_function_names(&self, _stack_id: &StackID) -> Vec<String> {
        unimplemented!("Not needed")
    }
}
