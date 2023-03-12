mod upload_function;

use actix_web::{
    guard,
    http::header::HeaderMap,
    services,
    web::{self, Json, PayloadConfig},
    HttpRequest,
};
use anyhow::{bail, Context, Result};
use api_common::{Request, Subject, SIGNATURE_HEADER_NAME, SUBJECT_HEADER_NAME};
use bytes::Bytes;
use ed25519_dalek::PublicKey;
use log::{debug, error};
use mu_gateway::HttpServiceFactoryBuilder;
use mu_stack::StackID;
use mu_storage::StorageClient;
use solana_sdk::pubkey::Pubkey;

use crate::stack::{request_signer_cache::RequestSignerCache, ApiRequestSigner};

const MAXIMUM_PAYLOAD_SIZE: usize = 10 * 1024 * 1024; // 10 Mebibyte

pub fn service_factory() -> impl HttpServiceFactoryBuilder {
    || {
        services![
            web::resource("/api")
                .app_data(PayloadConfig::new(MAXIMUM_PAYLOAD_SIZE))
                .guard(guard::All(guard::Post()).and(guard::fn_guard(|ctx| {
                    let headers = ctx.head().headers();
                    headers.contains_key(SUBJECT_HEADER_NAME)
                        && headers.contains_key(SIGNATURE_HEADER_NAME)
                })))
                .to(handle_request),
            web::resource("/api").to(|| async { bad_request("not found") })
        ]
    }
}

#[derive(Clone)]
pub struct DependencyAccessor {
    pub request_signer_cache: Box<dyn RequestSignerCache>,
    pub storage_client: Box<dyn StorageClient>,
}

async fn handle_request(
    request: HttpRequest,
    payload: Bytes,
    dependency_accessor: web::Data<DependencyAccessor>,
) -> Json<Result<api_common::Response, api_common::Error>> {
    debug!("Got new api request: {request:?}");

    let headers = request.headers();
    let Ok((subject, public_key)) = verify_signature(headers, &payload) else {
        return bad_request("can not verify request signature");
    };

    let Ok(request) = serde_json::from_slice::<Request>(&payload) else {
        return bad_request("can not deserialize request");
    };

    let Ok(_) = verify_subject_authority(&subject, &public_key, &dependency_accessor).await else {
        return bad_request("Unauthorized request");
    };

    Json(execute_request(dependency_accessor, request, subject).await)
}

fn bad_request<S: ToString>(reason: S) -> Json<Result<api_common::Response, api_common::Error>> {
    Json(Err(
        api_common::ServerError::BadRequest(reason.to_string()).into()
    ))
}

fn verify_signature(headers: &HeaderMap, payload: &[u8]) -> Result<(Subject, PublicKey)> {
    let subject_header = headers.get(SUBJECT_HEADER_NAME).context("")?;
    let subject = Subject::decode_base64(subject_header)?;

    let pubkey = match subject {
        Subject::User(pk) => pk,
        Subject::Stack { owner, .. } => owner,
    };
    let public_key = ed25519_dalek::PublicKey::from_bytes(&pubkey.to_bytes())?;

    let signature_header = headers.get(SIGNATURE_HEADER_NAME).context("")?;
    let signature_bytes = base64::decode(signature_header)?;
    let signature = ed25519_dalek::Signature::from_bytes(&signature_bytes[..])?;

    public_key.verify_strict(payload, &signature)?;
    Ok((subject, public_key))
}

async fn verify_subject_authority(
    subject: &Subject,
    public_key: &ed25519_dalek::PublicKey,
    dependency_accessor: &DependencyAccessor,
) -> Result<()> {
    match subject {
        Subject::User(_user_pubkey) => Ok(()), // Check user deposit account
        Subject::Stack { id, .. } => {
            verify_stack_ownership(id, &public_key, &dependency_accessor).await
        }
    }
}

async fn verify_stack_ownership(
    stack_id: &StackID,
    pubkey: &ed25519_dalek::PublicKey,
    dependency_accessor: &DependencyAccessor,
) -> Result<()> {
    let signer_key = Pubkey::new_from_array(*pubkey.as_bytes());
    match dependency_accessor
        .request_signer_cache
        .validate_signer(*stack_id, ApiRequestSigner::Solana(signer_key))
        .await
    {
        Err(e) => {
            error!("Failed to validate request signer: {e:?}");
            Err(e)
        }
        Ok(true) => Ok(()),
        Ok(false) => bail!("Invalid request signer key"),
    }
}

type ExecutionResult = std::result::Result<api_common::Response, api_common::Error>;

async fn execute_request(
    dependency_accessor: web::Data<DependencyAccessor>,
    request: Request,
    subject: Subject,
) -> ExecutionResult {
    match request {
        Request::Ping => Ok(api_common::Response::Ping),
        Request::UploadFunction(req) => {
            upload_function::execute(dependency_accessor, subject, req).await
        }
    }
}
