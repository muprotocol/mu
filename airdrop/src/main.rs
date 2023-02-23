mod config;
mod types;

use std::sync::Arc;

use actix_web::{
    dev::PeerAddr,
    post,
    web::{Data, Json},
    App, HttpServer,
};

use solana_client::nonblocking::rpc_client::RpcClient;
use solana_sdk::{signer::Signer, transaction::Transaction};
use types::{account_exists, AirdropRequest, AirdropResponse, Error, State};

async fn process_request(
    peer_addr: PeerAddr,
    request: &AirdropRequest,
    state: &State,
) -> Result<AirdropResponse, Error> {
    state.check_limits(peer_addr.0.ip(), request.to, request.amount)?;

    let token_account = spl_associated_token_account::get_associated_token_address(
        &request.to,
        &state.config.mint_pubkey,
    );

    if !account_exists(&state.solana_client, &token_account).await? {
        return Err(Error::TokenAccountNotInitializedYet);
    }

    let instruction = spl_token::instruction::mint_to(
        &spl_token::ID,
        &state.config.mint_pubkey,
        &token_account,
        &state.authority_keypair.pubkey(),
        &[&state.authority_keypair.pubkey()],
        request.amount,
    )
    .map_err(|e| Error::FailedToCreateTransaction(e.to_string()))?;

    let recent_blockhash = state
        .solana_client
        .get_latest_blockhash()
        .await
        .map_err(|e| Error::FailedToCreateTransaction(e.to_string()))?;

    let mut transaction =
        Transaction::new_with_payer(&[instruction], Some(&state.authority_keypair.pubkey()));
    transaction.sign(&[&state.authority_keypair], recent_blockhash);

    let result = if request.confirm_transaction {
        state
            .solana_client
            .send_and_confirm_transaction(&transaction)
            .await
    } else {
        state.solana_client.send_transaction(&transaction).await
    };

    result
        .map(|signature| AirdropResponse { signature })
        .map_err(|e| Error::FailedToSendTransaction(e.to_string()))
}

#[post("/airdrop")]
async fn request_airdrop(
    peer_addr: PeerAddr,
    request: Json<AirdropRequest>,
    app_data: Data<Arc<State>>,
) -> Json<Result<AirdropResponse, Error>> {
    let request = request.into_inner();
    let response = process_request(peer_addr, &request, &app_data).await;
    if let Err(ref error) = response {
        if let Err(revert_error) =
            app_data.revert_changes(peer_addr.0.ip(), request.to, request.amount)
        {
            return Json(Err(Error::Internal(format!(
                "Error while trying to recover from error:\nFirst Error: {:?}\nSecond Error: {:?}",
                error, revert_error,
            ))));
        }
    }

    Json(response)
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    let config = config::initialize_config().expect("initialize config");
    let authority_keypair = config.authority_keypair().expect("read authority keypair");
    let state = Arc::new(State {
        solana_client: RpcClient::new_socket(config.rpc_address),
        config: config.clone(),
        authority_keypair,
        cache: Default::default(),
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
