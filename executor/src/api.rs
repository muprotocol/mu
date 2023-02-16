use actix_web::{
    guard,
    http::header::HeaderMap,
    services,
    web::{self, Json},
    HttpRequest,
};
use anyhow::{bail, Context, Result};
use log::error;
use mu_gateway::HttpServiceFactoryBuilder;
use mu_stack::StackID;
use serde::{Deserialize, Serialize};
use serde_json::json;
use solana_sdk::pubkey::Pubkey;

use crate::stack::{request_signer_cache::RequestSignerCache, ApiRequestSigner};

const PUBLIC_KEY_HEADER_NAME: &str = "X-MU-PUBLIC-KEY";
const SIGNATURE_HEADER_NAME: &str = "X-MU-SIGNATURE";

pub fn service_factory() -> impl HttpServiceFactoryBuilder {
    || {
        services![
            web::resource("/api")
                .guard(guard::All(guard::Post()).and(guard::fn_guard(|ctx| {
                    let headers = ctx.head().headers();
                    headers.contains_key(PUBLIC_KEY_HEADER_NAME)
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
}

#[derive(Deserialize)]
pub struct ApiRequestTemplate {
    request: String,
    stack: String,
    params: serde_json::Value,
}

#[derive(Serialize)]
pub struct ApiResponseTemplate {
    params: serde_json::Value,
}

async fn handle_request(
    request: HttpRequest,
    payload: String,
    dependency_accessor: web::Data<DependencyAccessor>,
) -> (Json<serde_json::Value>, http::StatusCode) {
    async fn helper(
        request: HttpRequest,
        payload: String,
        dependency_accessor: web::Data<DependencyAccessor>,
    ) -> Result<(Json<serde_json::Value>, http::StatusCode)> {
        let headers = request.headers();
        let pubkey = verify_signature(headers, &payload)?;
        let request = serde_json::from_str::<ApiRequestTemplate>(payload.as_str())?;
        let stack_id = request.stack.parse::<StackID>()?;
        verify_stack_ownership(&stack_id, &pubkey, &dependency_accessor).await?;

        match execute_request(stack_id, request) {
            Ok(response) => Ok((Json(response), http::StatusCode::OK)),
            Err((response, status_code)) => Ok((Json(response), status_code)),
        }
    }

    helper(request, payload, dependency_accessor)
        .await
        .unwrap_or_else(|_| handle_bad_request())
}

fn verify_signature(headers: &HeaderMap, payload: &String) -> Result<ed25519_dalek::PublicKey> {
    let pubkey_header = headers.get(PUBLIC_KEY_HEADER_NAME).context("")?;
    let pubkey_bytes = base64::decode(pubkey_header)?;
    let pubkey = ed25519_dalek::PublicKey::from_bytes(&pubkey_bytes[..])?;
    let signature_header = headers.get(SIGNATURE_HEADER_NAME).context("")?;
    let signature_bytes = base64::decode(signature_header)?;
    let signature = ed25519_dalek::Signature::from_bytes(&signature_bytes[..])?;
    pubkey.verify_strict(payload.as_bytes(), &signature)?;
    Ok(pubkey)
}

fn handle_bad_request() -> (Json<serde_json::Value>, http::StatusCode) {
    let (j, s) = bad_request("bad request");
    (Json(j), s)
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

type ExecutionError = (serde_json::Value, http::StatusCode);
type ExecutionResult = std::result::Result<serde_json::Value, ExecutionError>;

fn bad_request(description: &'static str) -> ExecutionError {
    (json!(description), http::StatusCode::BAD_REQUEST)
}

fn execute_request(_stack_id: StackID, request: ApiRequestTemplate) -> ExecutionResult {
    match request.request.as_str() {
        "echo" => execute_echo(request.params),
        _ => Err(bad_request("unknown request")),
    }
}

fn execute_echo(params: serde_json::Value) -> ExecutionResult {
    let req = serde_json::from_value::<String>(params).map_err(|_| bad_request("invalid input"))?;
    Ok(json!(req))
}
