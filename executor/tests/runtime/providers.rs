use mu::runtime::FunctionProvider;
use std::{collections::HashMap, path::PathBuf};

/// Reads functions from filesystem in a `base_path` directory
/// # Example structure:
/// + base_path/
///     - ad73351d-2847-43b0-af1c-3ec81f8e5ad0/
///         - config.yaml
///         - module.wasm
///     - 069397a0-8bf5-4e16-9f1f-c10c671bb62d/
///         - config.yaml
///         - custom_module_name.wasm
///
/// Every function directory must contain a `config.yaml` file that defines the [`config`] for
/// the function and its module file name.
///
/// [`config`]: super::function::Config
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
                let definition = FunctionDefinition::new(id, src, config);
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
