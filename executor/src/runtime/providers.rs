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

// TODO: given the new CRUD operations on functions in this type, it's
// no longer necessary to have a trait. Remove it.
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

    fn remove_function(&mut self, id: &FunctionID) {
        self.functions
            .get_mut(&id.stack_id)
            .and_then(|f| f.remove(&id.function_name));
    }

    fn get_function_names(&self, stack_id: &StackID) -> Vec<String> {
        self.functions
            .get(&stack_id)
            .map(|f| f.keys().map(|s| s.clone()).collect())
            .unwrap_or_else(|| vec![])
    }
}
