use serde::Serialize;
use serde_json::json;

use super::{bad_request, ExecutionResult};

#[derive(Serialize)]
pub struct UploadFunctionRequest {}

fn execute_upload_function(params: serde_json::Value) -> ExecutionResult {
    let req = serde_json::from_value::<String>(params).map_err(|_| bad_request("invalid input"))?;
    Ok(json!(req))
}
