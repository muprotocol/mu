use async_trait::async_trait;
use mu::{mu_stack::StackID, runtime::types::*};
use std::collections::HashMap;

pub struct MapFunctionProvider {
    inner: HashMap<FunctionID, FunctionDefinition>,
}

impl MapFunctionProvider {
    pub fn new() -> Self {
        Self {
            inner: HashMap::new(),
        }
    }
}

#[async_trait]
impl FunctionProvider for MapFunctionProvider {
    fn get(&self, id: &FunctionID) -> Option<&FunctionDefinition> {
        Some(self.inner.get(id).unwrap())
    }

    fn add_function(&mut self, function: FunctionDefinition) {
        self.inner.insert(function.id.clone(), function);
    }

    fn remove_function(&mut self, id: &FunctionID) {
        self.inner.remove(id);
    }

    fn get_function_names(&self, _stack_id: &StackID) -> Vec<String> {
        unimplemented!("Not needed")
    }
}
