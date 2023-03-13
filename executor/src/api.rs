use actix_web::{
    guard,
    http::header::HeaderMap,
    services,
    web::{self, Json, PayloadConfig},
    HttpRequest,
};
use anyhow::Result;
use api_common::{
    requests::{EchoRequest, EchoResponse, UploadFunctionRequest, UploadFunctionResponse},
    ApiRequestTemplate, SIGNATURE_HEADER_NAME,
};
use log::error;
use mu_gateway::HttpServiceFactoryBuilder;
use mu_stack::StackOwner;
use mu_storage::StorageClient;
use serde::Deserialize;
use serde_json::json;

use crate::stack::blockchain_monitor::BlockchainMonitor;

pub const FUNCTION_STORAGE_NAME: &str = "FUNCTIONS";

pub fn service_factory(config: ApiConfig) -> impl HttpServiceFactoryBuilder {
    move || {
        services![
            web::resource("/api")
                .app_data(PayloadConfig::new(
                    config
                        .payload_size_limit
                        .get_bytes()
                        .try_into()
                        .unwrap_or(usize::MAX)
                ))
                .guard(guard::All(guard::Post()).and(guard::fn_guard(|ctx| {
                    let headers = ctx.head().headers();
                    headers.contains_key(SIGNATURE_HEADER_NAME)
                })))
                .to(handle_request),
            web::resource("/api").to(|| async { handle_bad_request() })
        ]
    }
}

#[derive(Clone)]
pub struct DependencyAccessor {
    //pub request_signer_cache: Box<dyn RequestSignerCache>,
    pub blockchain_monitor: Box<dyn BlockchainMonitor>,
    pub storage_client: Box<dyn StorageClient>,
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
    ) -> Result<Json<serde_json::Value>, Error> {
        let headers = request.headers();
        let request = serde_json::from_str::<ApiRequestTemplate>(payload.as_str())
            .map_err(|_| bad_request("can not deserialize request"))?;

        if let Some(owner) = request.user {
            let _pubkey = verify_signature(&owner, headers, &payload)?;
            //verify_stack_ownership(&stack_id, &pubkey, &dependency_accessor).await?; //TODO
            verify_escrow_account_balance(dependency_accessor.blockchain_monitor.clone(), &owner)
                .await?;
        } else {
            return bad_request("invalid signature");
        }

        execute_request(
            request.user,
            request,
            dependency_accessor.storage_client.clone(),
        )
        .await
        .map(Json)
    }

    match helper(request, payload, dependency_accessor).await {
        Ok(response) => (response, http::StatusCode::OK),
        Err((response, status_code)) => (Json(response), status_code),
    }
}

fn verify_signature(
    user: &StackOwner,
    headers: &HeaderMap,
    payload: &String,
) -> Result<ed25519_dalek::PublicKey, Error> {
    let pubkey = match user {
        StackOwner::Solana(pk) => ed25519_dalek::PublicKey::from_bytes(pk)
            .map_err(|_| internal_server_error("parsing pubkey"))?,
    };

    let signature_header = headers
        .get(SIGNATURE_HEADER_NAME)
        .ok_or_else(|| bad_request("signature header not found"))?;

    let signature_bytes = base64::decode(signature_header)
        .map_err(|_| bad_request("invalid base64 encoded signature"))?;

    let signature = ed25519_dalek::Signature::from_bytes(&signature_bytes[..])
        .map_err(|_| bad_request("invalid signature"))?;

    pubkey
        .verify_strict(payload.as_bytes(), &signature)
        .map_err(|_| bad_request("invalid signature"))?;

    Ok(pubkey)
}

async fn verify_escrow_account_balance(
    blockchain_monitor: Box<dyn BlockchainMonitor>,
    owner: &StackOwner,
) -> Result<(), Error> {
    match blockchain_monitor.get_escrow_balance(*owner).await {
        Ok(Some(balance)) => {
            if balance.is_over_minimum() {
                Ok(())
            } else {
                error!(
                    "Escrow account does not have enough balance, was: {}, minimum needed: {}",
                    balance.user_balance, balance.min_balance
                );
                Err(bad_request("Escrow account does not have enough balance"))
            }
        }

        Ok(None) => {
            error!("escrow account is not created yet");
            Err(bad_request("Escrow account is not created yet"))
        }

        Err(e) => {
            error!("can not check for escrow account balance: {e:?}");
            Err(internal_server_error("can not check escrow account"))
        }
    }
}

fn handle_bad_request() -> (Json<serde_json::Value>, http::StatusCode) {
    let (j, s) = bad_request("bad request");
    (Json(j), s)
}

//async fn verify_stack_ownership(
//    stack_id: &StackID,
//    pubkey: &ed25519_dalek::PublicKey,
//    dependency_accessor: &DependencyAccessor,
//) -> Result<()> {
//    let signer_key = Pubkey::new_from_array(*pubkey.as_bytes());
//    match dependency_accessor
//        .request_signer_cache
//        .validate_signer(*stack_id, ApiRequestSigner::Solana(signer_key))
//        .await
//    {
//        Err(e) => {
//            error!("Failed to validate request signer: {e:?}");
//            Err(e)
//        }
//        Ok(true) => Ok(()),
//        Ok(false) => bail!("Invalid request signer key"),
//    }
//}

type Error = (serde_json::Value, http::StatusCode);
type ExecutionResult = std::result::Result<serde_json::Value, Error>;

fn bad_request(description: &'static str) -> Error {
    (json!(description), http::StatusCode::BAD_REQUEST)
}

fn internal_server_error(description: &'static str) -> Error {
    (json!(description), http::StatusCode::INTERNAL_SERVER_ERROR)
}

async fn execute_request(
    user: Option<StackOwner>,
    request: ApiRequestTemplate,
    storage_client: Box<dyn StorageClient>,
) -> ExecutionResult {
    match request.request.as_str() {
        // "echo" => execute_echo(request.params),
        "upload_function" => execute_upload_function(request.params, user, storage_client).await,
        _ => Err(bad_request("unknown request")),
    }
}

// fn execute_echo(params: serde_json::Value) -> ExecutionResult {
//     let req =
//         serde_json::from_value::<EchoRequest>(params).map_err(|_| bad_request("invalid input"))?;

//     match serde_json::to_value(EchoResponse {
//         message: req.message,
//     }) {
//         Ok(r) => Ok(r),
//         Err(e) => {
//             error!("Failed to serialize response: {e:?}");
//             Err(internal_server_error("failed to serialize response"))
//         }
//     }
// }

async fn execute_upload_function(
    params: serde_json::Value,
    user: Option<StackOwner>,
    storage_client: Box<dyn StorageClient>,
) -> ExecutionResult {
    let Some(user) = user else {
        return Err(bad_request("this request needs user field"));
    };

    let req = serde_json::from_value::<UploadFunctionRequest>(params)
        .map_err(|_| bad_request("invalid input"))?;

    let Ok(bytes) = base64::decode(req.bytes) else {
        return Err(bad_request("invalid base64 encoded bytes"));
    };

    let file_id = base64::encode(stable_hash::fast_stable_hash(&bytes).to_be_bytes());
    let storage_owner = mu_storage::Owner::User(user);

    match storage_client
        .list(storage_owner, FUNCTION_STORAGE_NAME, &file_id)
        .await
    {
        Ok(l) if l.is_empty() => (),
        Ok(_) => {
            error!("Function upload failed, Use function is already uploaded into storage, file_id: {file_id}");
            return Err(bad_request("this file is already uploaded"));
        }
        Err(e) => {
            error!("Failed to upload user function in storage: {e:?}");
            return Err(internal_server_error("failed to upload function"));
        }
    }

    if let Err(e) = storage_client
        .put(
            storage_owner,
            FUNCTION_STORAGE_NAME,
            &file_id,
            &mut bytes.as_slice(),
        )
        .await
    {
        error!("Failed to upload user function in storage: {e:?}");
        return Err(internal_server_error("failed to upload function"));
    }

    match serde_json::to_value(UploadFunctionResponse { file_id }) {
        Ok(r) => Ok(r),
        Err(e) => {
            error!("Failed to serialize response: {e:?}");
            Err(internal_server_error("failed to serialize response"))
        }
    }
}

#[derive(Deserialize, Debug)]
pub struct ApiConfig {
    payload_size_limit: byte_unit::Byte,
}
