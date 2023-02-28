mod config;
mod database;
mod types;

use std::sync::Arc;

use actix_web::{
    dev::PeerAddr,
    post,
    web::{Data, Json},
    App, HttpServer,
};

use database::Database;
use log::{error, trace};
use solana_client::nonblocking::rpc_client::RpcClient;
use types::{fund_token_account, get_or_create_ata, AirdropRequest, AirdropResponse, Error, State};

async fn process_request(
    peer_addr: PeerAddr,
    request: &AirdropRequest,
    state: &State,
) -> Result<AirdropResponse, Error> {
    trace!("[{}] Got Request: {request:?}", request.to);

    state.check_limits(peer_addr.0.ip(), request.to, request.amount)?;

    let token_account = get_or_create_ata(state, &request.to).await?;

    let signature = fund_token_account(
        state,
        &token_account,
        request.amount,
        request.confirm_transaction,
    )
    .await?;

    let _ = state.database.insert_user(&request.email, &request.to);

    Ok(AirdropResponse { signature })
}

#[post("/airdrop")]
async fn request_airdrop(
    peer_addr: PeerAddr,
    request: Json<AirdropRequest>,
    app_data: Data<Arc<State>>,
) -> Json<Result<AirdropResponse, Error>> {
    let request = request.into_inner();
    let response = process_request(peer_addr, &request, &app_data).await;

    if let Err(Error::Internal(ref error)) = response {
        if let Err(revert_error) =
            app_data.revert_changes(peer_addr.0.ip(), request.to, request.amount)
        {
            error!(
                "Error while trying to recover from error:\nFirst Error: {:?}\nSecond Error: {:?}",
                error, revert_error
            );
            return Json(Err(Error::Internal("")));
        }
    }

    Json(response)
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    env_logger::init();

    let config = config::initialize_config().expect("initialize config");
    let authority_keypair = config.authority_keypair().expect("read authority keypair");
    let state = Arc::new(State {
        config: config.clone(),
        authority_keypair,
        cache: Default::default(),
        solana_client: RpcClient::new_socket(config.rpc_address),
        database: Database::open().expect("open database"),
    });

    HttpServer::new(move || {
        let state = state.clone(); //TODO: Don't use Arc, Data is using arc inside already
        App::new()
            .app_data(Data::new(state))
            .service(request_airdrop)
    })
    .bind(config.listen_address)?
    .run()
    .await
}
