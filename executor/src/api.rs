mod upload_function;

use actix_web::{
    guard,
    http::header::HeaderMap,
    services,
    web::{self, Json},
    HttpRequest,
};
use anyhow::{bail, Context, Result};
use api_common::{Request, ServerError, Subject, SIGNATURE_HEADER_NAME, SUBJECT_HEADER_NAME};
use bytes::Bytes;
use ed25519_dalek::PublicKey;
use log::error;
use mu_gateway::HttpServiceFactoryBuilder;
use mu_stack::StackID;
use mu_storage::StorageClient;
use reqwest::StatusCode;
use serde::Serialize;
use serde_json::json;
use solana_sdk::pubkey::Pubkey;

use crate::stack::{request_signer_cache::RequestSignerCache, ApiRequestSigner};

pub fn service_factory() -> impl HttpServiceFactoryBuilder {
    || {
        services![
            web::resource("/api")
                .guard(guard::All(guard::Post()).and(guard::fn_guard(|ctx| {
                    let headers = ctx.head().headers();
                    headers.contains_key(SUBJECT_HEADER_NAME)
                        && headers.contains_key(SIGNATURE_HEADER_NAME)
                })))
                .to(handle_request),
            web::resource("/api").to(|| async { handle_bad_request() })
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
) -> (Json<serde_json::Value>, http::StatusCode) {
    async fn helper(
        request: HttpRequest,
        payload: Bytes,
        dependency_accessor: web::Data<DependencyAccessor>,
    ) -> Result<(Json<serde_json::Value>, http::StatusCode)> {
        let headers = request.headers();
        let (subject, public_key) = verify_signature(headers, &payload)?;
        let request = serde_json::from_slice::<Request>(&payload)?;

        verify_subject_authority(&subject, &public_key, &dependency_accessor).await?;

        match execute_request(dependency_accessor, request, subject).await {
            Ok(response) => Ok((Json(response), http::StatusCode::OK)),
            Err((response, status_code)) => Ok((Json(response), status_code)),
        }
    }

    helper(request, payload, dependency_accessor)
        .await
        .unwrap_or_else(|_| handle_bad_request())
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

fn handle_bad_request() -> (Json<serde_json::Value>, http::StatusCode) {
    let (j, s) = bad_request("bad request");
    (Json(j), s)
}

fn bad_request(description: &'static str) -> ExecutionError {
    (json!(description), http::StatusCode::BAD_REQUEST)
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

type ExecutionResult = std::result::Result<serde_json::Value, ExecutionError>;
type ExecutionError = (serde_json::Value, http::StatusCode);

async fn execute_request(
    dependency_accessor: web::Data<DependencyAccessor>,
    request: Request,
    subject: Subject,
) -> ExecutionResult {
    fn helper<T: Serialize>(output: T) -> ExecutionResult {
        match serde_json::to_value(output) {
            Ok(r) => Ok(r),
            Err(e) => {
                error!("Can not serialize response: {e:?}");
                Err((
                    serde_json::to_value(ServerError::FailedToSerializeResponse).unwrap(), //TODO: what else?
                    StatusCode::INTERNAL_SERVER_ERROR,
                ))
            }
        }
    }

    match request {
        Request::Ping => helper(Ok::<serde_json::Value, ()>(json! {"pong"})),
        Request::UploadFunction(req) => {
            helper(upload_function::execute(dependency_accessor, subject, req).await)
        }
    }
}
