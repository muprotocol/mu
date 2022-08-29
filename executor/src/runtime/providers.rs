use super::types::{FunctionDefinition, FunctionID, FunctionProvider};
use crate::mu_stack::StackID;
use std::collections::HashMap;

type FunctionName = String;

pub struct DefaultFunctionProvider {
    functions: HashMap<StackID, HashMap<FunctionName, FunctionDefinition>>,
}

impl Default for DefaultFunctionProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl DefaultFunctionProvider {
    pub fn new() -> Self {
        Self {
            functions: HashMap::new(),
        }
    }
}

impl FunctionProvider for DefaultFunctionProvider {
    fn get(&self, id: &FunctionID) -> Option<&FunctionDefinition> {
        self.functions
            .get(&id.stack_id)
            .and_then(|f| f.get(&id.function_name))
    }

    fn add_function(&mut self, function: super::types::FunctionDefinition) {
        let id = &function.id;
        let stack_functions = self.functions.entry(id.stack_id).or_insert(HashMap::new());
        stack_functions.insert(id.function_name.clone(), function);
    }
}
