//TODO
#![allow(dead_code)]

use anyhow::Result;
use error::Error;
use function::{Config, Function, InstanceID};
use std::{collections::HashMap, time::Duration};
use uuid::Uuid;

use message::gateway::{GatewayRequest, GatewayResponse};

pub mod error;
mod function;
pub mod message;
mod mock;

//TODO: use metrics and MemoryUsage so we can report usage of memory and CPU time.
#[derive(Default)]
pub struct Runtime {
    //TODO: use Vec<Function> and hold more than one function at a time so we can load balance
    // over funcs.
    instances: HashMap<InstanceID, Function>,
}

impl Runtime {
    async fn load_function(&mut self, config: Config) -> Result<()> {
        if self.instances.get(&config.id).is_none() {
            let id = config.id;
            let function = Function::load(config).await?;
            self.instances.insert(id, function);
        }
        Ok(())
    }

    async fn run_with_gateway_request(
        &mut self,
        id: Uuid,
        _request: GatewayRequest,
    ) -> Result<GatewayResponse> {
        if let Some(_f) = self.instances.get_mut(&id) {
            //let output = f.run(request).await?;
            //GatewayResponse::parse(output)?
            todo!()
        } else {
            Err(Error::FunctionNotFound(id).into())
        }
    }

    pub async fn listen(&mut self) {
        let mut gateway = mock::gateway::start(Duration::from_secs(1), 10, |i| {
            GatewayRequest::new(rand::random(), format!("Test Request Number {}", i))
        })
        .await;

        while let Some((_request, _resposne_sender)) = gateway.recv().await {
            todo!();
        }
    }
}
