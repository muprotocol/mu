use async_trait::async_trait;
use mu::runtime::types::*;
use mu_stack::StackID;
use std::collections::HashMap;

pub struct MapAssemblyProvider {
    inner: HashMap<AssemblyID, AssemblyDefinition>,
}

impl MapAssemblyProvider {
    pub fn new() -> Self {
        Self {
            inner: HashMap::new(),
        }
    }
}

#[async_trait]
impl AssemblyProvider for MapAssemblyProvider {
    fn get(&self, id: &AssemblyID) -> Option<&AssemblyDefinition> {
        Some(self.inner.get(id).unwrap())
    }

    fn add_function(&mut self, function: AssemblyDefinition) {
        self.inner.insert(function.id.clone(), function);
    }

    fn remove_function(&mut self, id: &AssemblyID) {
        self.inner.remove(id);
    }

    fn get_function_names(&self, _stack_id: &StackID) -> Vec<String> {
        unimplemented!("Not needed")
    }
}
