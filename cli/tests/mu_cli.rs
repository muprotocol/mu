mod utils;

use clap::Parser;
use mu_cli::{entry, Opts};
use utils::create_wallet_and_associated_token_account;

#[test]
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

    let opts = Opts::try_parse_from(args).unwrap();
    entry(opts).unwrap();

    //TODO: check if provider is created successfully.
}

//#[test]
//fn can_create_stack() {
//    unimplemented!()
//}
//
//#[test]
//fn can_create_region() {
//    unimplemented!()
//}
//
//#[test]
//fn can_create_authorized_usage_signer() {
//    unimplemented!()
//}
//
//#[test]
//fn can_update_usage() {
//    unimplemented!()
//}
