mod config;
mod types;

use actix_web::{post, web::Json, App, HttpServer};

use types::{AirdropRequest, AirdropResponse, Error};

#[post("/airdrop")]
async fn request_airdrop(request: Json<AirdropRequest>) -> Json<Result<AirdropResponse, Error>> {
    todo!()
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    let 
    HttpServer::new(|| App::new().service(request_airdrop))
        .bind(("127.0.0.1", 8080))?
        .run()
        .await
}
