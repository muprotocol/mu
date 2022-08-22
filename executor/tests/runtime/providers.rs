use anyhow::{bail, Context, Result};
use async_trait::async_trait;
use mu::runtime::{
    function::{Config, FunctionDefinition, FunctionID},
    FunctionProvider,
};
use std::{
    collections::{hash_map::Entry, HashMap},
    path::PathBuf,
};

/// Reads functions from filesystem in a `base_path` directory
/// # Example structure:
/// + base_path/
///     - 4e955fea0268518cbaa500409dfbec88f0ecebad28d84ecbe250baed97dba889/
///         - config.yaml
///         - module.wasm
///     - 6ffb5aeee54da02896f0d8585e8ef0c80f574d0492e2d15b9ea6bc3a34b1245e/
///         - config.yaml
///         - custom_module_name.wasm
///
/// Every function directory must contain a `config.yaml` file that defines the [`config`] for
/// the function and its module file name.
///
/// [`config`]: mu::runtime::function:Config
pub struct DiskFunctionProvider {
    base_path: PathBuf,
    functions: HashMap<FunctionID, FunctionDefinition>,
}

#[async_trait]
impl FunctionProvider for DiskFunctionProvider {
    async fn get(&mut self, id: FunctionID) -> Result<&FunctionDefinition> {
        if let Entry::Vacant(e) = self.functions.entry(id) {
            let function_dir = self.base_path.join(id.to_string());
            if function_dir
                .try_exists()
                .context("function directory not found")?
            {
                let config_path = function_dir.join("config.yaml");
                let config = tokio::fs::read(config_path).await?;
                let config: Config = serde_yaml::from_slice(&config)?;

                let src = tokio::fs::read(function_dir.join(&config.module_path)).await?;
                let definition = FunctionDefinition::new(src, config);
                e.insert(definition);
                Ok(self.functions.get(&id).unwrap()) //This is safe, we just imported the new item
            } else {
                bail!("Function not found: {id}")
            }
        } else {
            Ok(self.functions.get(&id).unwrap())
        }
    }
}

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
    async fn get(&mut self, id: FunctionID) -> Result<&FunctionDefinition> {
        Ok(self.inner.get(&id).unwrap())
    }
}
