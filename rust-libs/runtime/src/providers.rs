use std::collections::HashMap;

use super::types::AssemblyDefinition;
use mu_stack::{AssemblyID, StackID};

type FunctionName = String;

pub struct AssemblyProvider {
    functions: HashMap<StackID, HashMap<FunctionName, AssemblyDefinition>>,
}

impl Default for AssemblyProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl AssemblyProvider {
    pub fn new() -> Self {
        Self {
            functions: HashMap::new(),
        }
    }

    pub fn get(&self, id: &AssemblyID) -> Option<&AssemblyDefinition> {
        self.functions
            .get(&id.stack_id)
            .and_then(|f| f.get(&id.assembly_name))
    }

    pub fn add_function(&mut self, assembly: super::types::AssemblyDefinition) {
        let id = &assembly.id;
        let stack_functions = self
            .functions
            .entry(id.stack_id)
            .or_insert_with(HashMap::new);
        stack_functions.insert(id.assembly_name.clone(), assembly);
    }

    pub fn remove_function(&mut self, id: &AssemblyID) {
        self.functions
            .get_mut(&id.stack_id)
            .and_then(|f| f.remove(&id.assembly_name));
    }

    pub fn remove_all_functions(&mut self, stack_id: &StackID) -> Option<Vec<String>> {
        self.functions
            .remove(stack_id)
            .map(|map| map.into_keys().collect::<Vec<_>>())
    }

    pub fn get_function_names(&self, stack_id: &StackID) -> Vec<String> {
        self.functions
            .get(stack_id)
            .map(|f| f.keys().cloned().collect())
            .unwrap_or_else(Vec::new)
    }
}
