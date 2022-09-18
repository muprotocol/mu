//! A

use {
    anyhow::Result,
    clap::{
        crate_authors, crate_description, crate_name, crate_version, value_t, App, Arg, ArgMatches,
        SubCommand,
    },
    solana_clap_utils::keypair::signer_from_path,
    solana_remote_wallet::remote_wallet::maybe_wallet_manager,
    std::{ffi::OsString, process::exit},
};

use crate::{
    cli::{Args, Command},
    config::MuCliConfig,
};

fn get_matches<'a, I, T>(args: I) -> ArgMatches<'a>
where
    I: IntoIterator<Item = T>,
    T: Into<OsString> + Clone,
{
    App::new(crate_name!())
        .author(crate_authors!())
        .about(crate_description!())
        .version(crate_version!())
        .arg(
            Arg::with_name("cluster_url")
                .long("cluster-url")
                .global(true)
                .takes_value(true)
                .value_name("Cluster URL")
                .help("RPC entrypoint address. i.e. http://api.devnet.solana.com"),
        )
        .subcommand(
            SubCommand::with_name("provider")
                .about("Provider specific operations")
                .subcommand(
                    SubCommand::with_name("create")
                        .about("Create a new provider")
                        .arg(
                            Arg::with_name("name")
                                .long("name")
                                .required(true)
                                .takes_value(true)
                                .value_name("NAME")
                                .help("The name for new provider"),
                        ),
                ),
        )
        .get_matches_from(args)
}

/// Parse CLI arguments
pub fn parse_args_and_config<I, T>(args: I) -> Result<(Args, MuCliConfig)>
where
    I: IntoIterator<Item = T>,
    T: Into<OsString> + Clone,
{
    use crate::commands::*;

    let matches = get_matches(args);
    let mut wallet_manager = maybe_wallet_manager()?;

    let config = crate::config::MuCliConfig::initialize()?;

    let keypair_str = match value_t!(matches, "keypair", String) {
        Ok(p) => p,
        Err(e) if e.kind == clap::ErrorKind::ArgumentNotFound => config
            .keypair_path
            .clone()
            .ok_or(anyhow!("keypair is required"))?,
        Err(e) => return Err(e.into()),
    };
    let keypair = signer_from_path(&matches, &keypair_str, "sender", &mut wallet_manager)
        .map_err(|e| anyhow!("Error while reading keypair {e} "))?;

    let command = match matches.subcommand() {
        ("provider", Some(matches)) => Command::Provider(provider::parse(matches)?),
        _ => {
            eprintln!("{}", matches.usage());
            exit(1);
        }
    };

    let args = Args { keypair, command };
    Ok((args, config))
}
