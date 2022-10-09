mod utils;

use clap::Parser;
use mu_cli::{entry, Opts};
use utils::program::setup_env;

#[test]
fn can_create_provider() {
    let (mu_program, _validator_handler) = setup_env().unwrap();

    let (provider_wallet, _associated_token_account_address) = mu_program
        .create_wallet_and_associated_token_account()
        .unwrap();
    let provider_wallet_path = provider_wallet.path.display().to_string();

    let args = vec![
        "mu",
        "--wallet",
        &provider_wallet_path,
        "--cluster",
        "localnet",
        "provider",
        "create",
        "--name",
        "SomeProvider",
    ];

    let opts = Opts::try_parse_from(args).unwrap();
    entry(opts).unwrap();
}

#[test]
fn can_create_stack() {
    unimplemented!()
}

#[test]
fn can_create_region() {
    unimplemented!()
}

#[test]
fn can_create_authorized_usage_signer() {
    unimplemented!()
}

#[test]
fn can_update_usage() {
    unimplemented!()
}
