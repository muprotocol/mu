mod config;
mod types;

use actix_web::{
    dev::PeerAddr,
    post,
    web::{Data, Json},
    App, HttpServer,
};

use solana_client::nonblocking::rpc_client::RpcClient;
use solana_sdk::{signer::Signer, transaction::Transaction};
use types::{AirdropRequest, AirdropResponse, Error, State};

#[post("/airdrop")]
async fn request_airdrop(
    peer_addr: PeerAddr,
    request: Json<AirdropRequest>,
    app_data: Data<State>,
) -> Json<Result<AirdropResponse, Error>> {
    let request = request.into_inner();

    if let Err(e) = app_data.check_limits(peer_addr.0.ip(), request.to, request.amount) {
        return Json(Err(e));
    }

    let token_account = spl_associated_token_account::get_associated_token_address(
        &request.to,
        &app_data.config.mint_pubkey,
    );

    let solana_client = RpcClient::new_socket(app_data.config.rpc_address);

    let instruction = match spl_token::instruction::mint_to(
        &spl_token::ID,
        &app_data.config.mint_pubkey,
        &token_account,
        &app_data.authority_keypair.pubkey(),
        &[&app_data.authority_keypair.pubkey()],
        request.amount,
    ) {
        Err(e) => return Json(Err(Error::FailedToCreateTransaction(e.to_string()))),
        Ok(ins) => ins,
    };

    let recent_blockhash = match solana_client.get_latest_blockhash().await {
        Err(e) => return Json(Err(Error::FailedToCreateTransaction(e.to_string()))),
        Ok(hash) => hash,
    };

    let mut transaction =
        Transaction::new_with_payer(&[instruction], Some(&app_data.authority_keypair.pubkey()));
    transaction.sign(&[&app_data.authority_keypair], recent_blockhash);

    let result = if request.confirm_transaction {
        solana_client
            .send_and_confirm_transaction(&transaction)
            .await
    } else {
        solana_client.send_transaction(&transaction).await
    };

    let signature = match result {
        Err(e) => return Json(Err(Error::FailedToSendTransaction(e.to_string()))),
        Ok(sig) => sig,
    };

    println!("{} Requested {} MU", request.to, request.amount);
    Json(Ok(AirdropResponse { signature }))
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    let config = config::initialize_config().expect("initialize config");
    HttpServer::new({
        let config = config.clone();
        move || {
            let config = config.clone();
            App::new()
                .app_data({
                    let authority_keypair =
                        config.authority_keypair().expect("read authority keypair");
                    let state = State {
                        config,
                        authority_keypair,
                        cache: Default::default(),
                    };

                    Data::new(state)
                })
                .service(request_airdrop)
        }
    })
    .bind(config.listen_address)?
    .run()
    .await
}
