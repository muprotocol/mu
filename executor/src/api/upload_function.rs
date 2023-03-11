use actix_web::web;
use api_common::{requests::UploadFunctionRequest, Subject};
use sha2::{Digest, Sha256};

use super::{DependencyAccessor, ExecutionResult};

pub async fn execute(
    dependency_accessor: web::Data<DependencyAccessor>,
    subject: Subject,
    request: UploadFunctionRequest,
) -> ExecutionResult {
    let mut hasher = Sha256::new();
    hasher.update(subject.pubkey());
    hasher.update(request.bytes);

    let file_id = base64::encode(hasher.finalize());
    dependency_accessor.storage_client.put(stack_id, storage_name, key, reader)

    todo!()
}
