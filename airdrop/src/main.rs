mod config;
mod database;
mod marketplace;
mod types;

use std::sync::Arc;

use actix_cors::Cors;
use actix_web::{
    dev::PeerAddr,
    http, post,
    web::{Data, Json},
    App, HttpServer,
};

use log::trace;
use types::{fund_token_account, get_or_create_ata, AirdropRequest, AirdropResponse, Error, State};

async fn process_request(
    peer_addr: PeerAddr,
    request: &AirdropRequest,
    state: &State,
) -> Result<AirdropResponse, Error> {
    trace!("[{}] Got Request: {request:?}", request.to);

    state.check_limits(peer_addr.0.ip(), request.to, request.amount)?;

    let token_account = get_or_create_ata(state, &request.to).await?;

    let signature = fund_token_account(state, &token_account, request.amount).await?;

    let _ = state.database.insert_user(&request.email, &request.to);

    Ok(AirdropResponse { signature })
}

#[post("/airdrop")]
async fn request_airdrop(
    peer_addr: PeerAddr,
    request: Json<AirdropRequest>,
    app_data: Data<Arc<State>>,
) -> (Json<Result<AirdropResponse, Error>>, http::StatusCode) {
    let request = request.into_inner();
    let response = process_request(peer_addr, &request, &app_data).await;

    if let Err(Error::FailedToProcessTransaction) = response {
        let _ = app_data.revert_changes(peer_addr.0.ip(), request.to, request.amount);
    }

    match response {
        x @ Ok(_) => (Json(x), http::StatusCode::OK),
        x @ Err(_) => (Json(x), http::StatusCode::BAD_REQUEST),
    }
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    env_logger::init();

    let config = config::initialize_config().expect("initialize config");
    let state = Arc::new(State::init(config.clone()).await.expect("initialize state"));

    HttpServer::new(move || {
        let state = state.clone(); //TODO: Don't use Arc, Data is using arc inside already

        let cors = Cors::default()
            .allow_any_origin()
            .allowed_methods(vec!["POST"])
            .allowed_headers(vec![http::header::ACCEPT])
            .allowed_header(http::header::CONTENT_TYPE)
            .max_age(3600);

        App::new()
            .wrap(cors)
            .app_data(Data::new(state))
            .service(request_airdrop)
    })
    .bind(config.listen_address)?
    .run()
    .await
}
