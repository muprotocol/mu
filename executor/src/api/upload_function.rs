use actix_web::web;
use api_common::Subject;
use serde_json::json;

use api_common::request::UploadFunctionRequest;

use super::{bad_request, DependencyAccessor, ExecutionResult};

pub fn execute(
    dependency_accessor: web::Data<DependencyAccessor>,
    subject: Subject,
    params: serde_json::Value,
) -> ExecutionResult {
    let req = serde_json::from_value::<String>(params).map_err(|_| bad_request("invalid input"))?;
    Ok(json!(req))
}
