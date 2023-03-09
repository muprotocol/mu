use api_common::Subject;
use serde::Serialize;
use serde_json::json;

use super::{bad_request, ExecutionResult};

#[derive(Serialize)]
pub struct UploadFunctionRequest {}

pub fn execute(subject: Subject, params: serde_json::Value) -> ExecutionResult {
    let req = serde_json::from_value::<String>(params).map_err(|_| bad_request("invalid input"))?;
    Ok(json!(req))
}
