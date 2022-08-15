//TODO
#![allow(dead_code)]
//TODO: Add logging

pub mod error;
mod function;
mod message;
mod providers;

use self::function::{FunctionDefinition, FunctionID, FunctionIO};
use anyhow::Result;
use async_trait::async_trait;
use std::collections::HashMap;
use tokio::task::JoinHandle;
use uuid::Uuid;

/// This is FunctionProvider that should cache functions if needed.
#[async_trait]
pub trait FunctionProvider {
    async fn get(&mut self, id: FunctionID) -> anyhow::Result<&FunctionDefinition>;
}

//TODO:
// * use metrics and MemoryUsage so we can report usage of memory and CPU time.
// * remove less frequently used source's from runtime
// * hold more than one instance of functions and load balance on them
pub struct Runtime<P: FunctionProvider> {
    instances: HashMap<FunctionID, Instance>,
    function_provider: P,
}

impl<P> Runtime<P>
where
    P: FunctionProvider,
{
    pub fn new(provider: P) -> Self {
        Self {
            instances: HashMap::new(),
            function_provider: provider,
        }
    }

    //TODO: check and maintain function status better
    async fn run_function(&mut self, id: FunctionID) -> Result<()> {
        match self.instances.get(&id) {
            Some(i) if !i.is_finished() => (),
            _ => {
                let definition = self.function_provider.get(id).await?;
                let instance = Instance::new(definition).await?;
                self.instances.insert(id, instance);
            }
        }
        Ok(())
    }

    pub async fn start(&mut self) {
        loop {}
    }
}

pub struct Instance {
    id: Uuid,
    io: FunctionIO,
    join_handle: JoinHandle<()>,
}

impl Instance {
    pub async fn new(definition: &FunctionDefinition) -> Result<Self> {
        let function = definition.create_function().await?;
        let (join_handle, io) = function.start()?;
        Ok(Self {
            id: definition.id,
            io,
            join_handle,
        })
    }

    pub fn is_finished(&self) -> bool {
        self.join_handle.is_finished()
    }
}
