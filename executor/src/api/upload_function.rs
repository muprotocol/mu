use actix_web::web;
use api_common::{
    requests::{UploadFunctionRequest, UploadFunctionResponse},
    ServerError, Subject,
};
use log::error;

use super::{DependencyAccessor, ExecutionResult};

const FUNCTION_STORAGE_NAME: &str = "FUNCTIONS";

pub async fn execute(
    dependency_accessor: web::Data<DependencyAccessor>,
    subject: Subject,
    request: UploadFunctionRequest,
) -> ExecutionResult {
    let file_id = base64::encode(stable_hash::fast_stable_hash(&request.bytes).to_be_bytes());

    let storage_owner = match subject {
        Subject::User(pk) => mu_storage::Owner::User(pk),
        Subject::Stack { .. } => {
            return Err(ServerError::UnexpectedSubject("User".into(), "Stack".into()).into())
        }
    };

    if let Err(e) = dependency_accessor
        .storage_client
        .put(
            storage_owner,
            FUNCTION_STORAGE_NAME,
            &file_id,
            &mut request.bytes.as_slice(),
        )
        .await
    {
        error!("Failed to upload user function in storage: {e:?}");
        return Err(ServerError::UploadFunction.into());
    }

    Ok(api_common::Response::UploadFunction(
        UploadFunctionResponse { file_id },
    ))
}
