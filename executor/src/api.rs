use actix_web::{
    guard, services,
    web::{self, Json},
    HttpRequest,
};
use anyhow::Context;
use mu_gateway::HttpServiceFactoryBuilder;
use mu_stack::StackID;
use serde::{Deserialize, Serialize};
use solana_sdk::pubkey::Pubkey;

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
            web::resource("/api").to(bad_request)
        ]
    }
}

pub struct DependencyAccessor {
    api_signer_cache: todo,
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

// TODO: bad idea.
macro_rules! ok_or_bad_request {
    ([$($var:ident = $e:expr,)+]) => {
        $(let Ok($var) = $e else { return (actix_web::web::Json(serde_json::Value::String("bad_request".into())), http::StatusCode::BAD_REQUEST); };)*
    };
}

pub async fn handle_request(
    request: HttpRequest,
    payload: String,
) -> (Json<serde_json::Value>, http::StatusCode) {
    let headers = request.headers();
    ok_or_bad_request!([
        pubkey_header = headers.get(PUBLIC_KEY_HEADER_NAME).context(""),
        pubkey_bytes = base64::decode(pubkey_header),
        pubkey = ed25519_dalek::PublicKey::from_bytes(&pubkey_bytes[..]),
        signature_header = headers.get(SIGNATURE_HEADER_NAME).context(""),
        signature_bytes = base64::decode(signature_header),
        signature = ed25519_dalek::Signature::from_bytes(&signature_bytes[..]),
        _verify_signature_result = pubkey.verify_strict(payload.as_bytes(), &signature),
        request = serde_json::from_str::<ApiRequestTemplate>(payload.as_str()),
        stack_id = request.stack.parse::<StackID>(),
        _verify_stack_ownership_result = verify_stack_ownership(&stack_id, &pubkey),
    ]);
    (Json(request.params), http::StatusCode::OK)
}

pub async fn bad_request() -> (Json<serde_json::Value>, http::StatusCode) {
    (
        Json(serde_json::Value::String("bad request".into())),
        http::StatusCode::BAD_REQUEST,
    )
}

async fn verify_stack_ownership(
    stack_id: &StackID,
    pubkey: &ed25519_dalek::PublicKey,
) -> Result<()> {
    let requester_key = Pubkey::new_from_array(pubkey.as_bytes());
}
