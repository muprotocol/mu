mod utils;

use mu_cli::arg_parser::parse_args_and_config;
use utils::program::setup_env;

#[test]
fn can_create_provider() {
    let (mu_program, _validator_handler) = setup_env().unwrap();
    let args = vec!["mu", "provider", "create", "--name", "SomeProvider"];

    let (provider_wallet, associated_token_account_address) = mu_program
        .create_wallet_and_associated_token_account()
        .unwrap();

    parse_args_and_config(args).unwrap().execute().unwrap();
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
