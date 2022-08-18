use super::{
    function::{FunctionDefinition, FunctionID},
    FunctionProvider,
};
use anyhow::Result;
use async_trait::async_trait;
use reqwest::Url;
use std::collections::{hash_map::Entry, HashMap};

pub struct FunctionProviderImpl {
    base_url: Url, //TODO: use concrete Uri type
    functions: HashMap<FunctionID, FunctionDefinition>,
}

#[async_trait]
impl FunctionProvider for FunctionProviderImpl {
    async fn get(&mut self, id: FunctionID) -> Result<&FunctionDefinition> {
        if let Entry::Vacant(e) = self.functions.entry(id) {
            //let file_uri = format!("{}/{}.tar.gz", self.base_url, id);
            //let a = reqwest::get(file_uri).await?.bytes().await?;
            //if function_dir
            //    .try_exists()
            //    .context("function directory not found")?
            //{
            //    let config_path = function_dir.join("config.yaml");
            //    let config = tokio::fs::read(config_path).await?;
            //    let config: Config = serde_yaml::from_slice(&config)?;

            //    let src = tokio::fs::read(function_dir.join(&config.module_path)).await?;
            //    let definition = FunctionDefinition::new(id, src, config);
            //    e.insert(definition);
            //    Ok(self.functions.get(&id).unwrap()) //This is safe, we just imported the new item
            //} else {
            //    bail!("Function not found: {id}")
            //}
            todo!()
        } else {
            Ok(self.functions.get(&id).unwrap())
        }
    }
}
