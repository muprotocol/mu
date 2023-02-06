use std::collections::HashMap;

use super::types::{AssemblyDefinition, AssemblyProvider};
use mu_stack::{AssemblyID, StackID};

type FunctionName = String;

pub struct DefaultAssemblyProvider {
    functions: HashMap<StackID, HashMap<FunctionName, AssemblyDefinition>>,
}

impl Default for DefaultAssemblyProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl DefaultAssemblyProvider {
    pub fn new() -> Self {
        Self {
            functions: HashMap::new(),
        }
    }
}

// TODO: given the new CRUD operations on functions in this type, it's
// no longer necessary to have a trait. Remove it.
impl AssemblyProvider for DefaultAssemblyProvider {
    fn get(&self, id: &AssemblyID) -> Option<&AssemblyDefinition> {
        self.functions
            .get(&id.stack_id)
            .and_then(|f| f.get(&id.assembly_name))
    }

    fn add_function(&mut self, assembly: super::types::AssemblyDefinition) {
        let id = &assembly.id;
        let stack_functions = self
            .functions
            .entry(id.stack_id)
            .or_insert_with(HashMap::new);
        stack_functions.insert(id.assembly_name.clone(), assembly);
    }

    fn remove_function(&mut self, id: &AssemblyID) {
        self.functions
            .get_mut(&id.stack_id)
            .and_then(|f| f.remove(&id.assembly_name));
    }

    fn remove_all_functions(&mut self, stack_id: &StackID) -> Option<Vec<String>> {
        self.functions
            .remove(stack_id)
            .map(|map| map.into_keys().collect::<Vec<_>>())
    }

    fn get_function_names(&self, stack_id: &StackID) -> Vec<String> {
        self.functions
            .get(stack_id)
            .map(|f| f.keys().cloned().collect())
            .unwrap_or_else(Vec::new)
    }
}
