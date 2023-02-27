mod utils;

use clap::Parser;
use mu_cli::{execute, Arguments};
use utils::create_wallet_and_associated_token_account;

#[test]
#[ignore = "Need fully functional Solana environment to run this test, not available in CI"]
fn can_create_provider() {
    let (provider_wallet, _) = create_wallet_and_associated_token_account().unwrap();
    let provider_wallet_path = provider_wallet.path.display().to_string();

    let args = vec![
        "mu",
        "--payer",
        &provider_wallet_path,
        "--cluster",
        "localnet",
        "provider",
        "create",
        "--name",
        "SomeProvider",
        "--provider-keypair",
        &provider_wallet_path,
    ];

    let opts = Arguments::try_parse_from(args).unwrap();
    execute(opts).unwrap();

    //TODO: check if provider is created successfully.
}

#[test]
#[ignore = "Not Implemented"]
fn can_create_stack() {
    unimplemented!()
}

#[test]
#[ignore = "Not Implemented"]
fn can_create_region() {
    unimplemented!()
}

#[test]
#[ignore = "Not Implemented"]
fn can_create_authorized_usage_signer() {
    unimplemented!()
}

#[test]
#[ignore = "Not Implemented"]
fn can_update_usage() {
    unimplemented!()
}
