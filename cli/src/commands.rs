use std::{collections::HashMap, path::Path};

use anyhow::{Context, Result};
use beau_collector::BeauCollector;
use clap::{Args, Parser};
use tokio::select;
use tokio_util::sync::CancellationToken;

use crate::{
    config::{Config, ConfigOverride},
    runtime, template,
};

pub mod escrow;
pub mod list;
pub mod provider;
pub mod stack;

pub const VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Debug, Parser)]
pub enum Command {
    /// Provider management. If you're a developer, this is not what you're looking for.
    Provider {
        #[command(subcommand)]
        sub_command: provider::Command,
    },

    List {
        #[command(subcommand)]
        sub_command: list::Command,
    },

    Escrow {
        #[command(subcommand)]
        sub_command: escrow::Command,
    },

    Stack {
        #[command(subcommand)]
        sub_command: stack::Command,
    },

    /// Initialize a new mu project
    Init(InitCommand),

    /// Build mu project
    Build,

    /// Run mu project
    Run,
}

#[derive(Debug, Args)]
pub struct InitCommand {
    /// Initialize a new mu project.
    name: String,

    /// The directory to create new project in.
    path: Option<String>,

    #[arg(short, long)]
    /// Template to use for new project.
    template: String,

    #[arg(short, long)]
    /// Language.
    language: Option<String>,
}

#[derive(Debug, Parser)]
#[clap(version = VERSION, about)]
pub struct Arguments {
    #[command(flatten)]
    pub cfg_override: ConfigOverride,
    #[command(subcommand)]
    pub command: Command,
}

pub async fn execute(args: Arguments) -> Result<()> {
    let config = Config::discover(args.cfg_override)?;
    match args.command {
        Command::Provider { sub_command } => provider::execute(config, sub_command),
        Command::List { sub_command } => list::execute(config, sub_command),
        Command::Escrow { sub_command } => escrow::execute(config, sub_command),
        Command::Stack { sub_command } => stack::execute(config, sub_command),

        Command::Init(sub_command) => execute_init(config, sub_command),
        Command::Build => execute_build(config),
        Command::Run => execute_run(config).await,
    }
}

pub fn execute_init(_config: Config, cmd: InitCommand) -> Result<()> {
    let templates = template::read_templates()?;

    match templates.iter().find(|t| {
        t.name == cmd.template && {
            match &cmd.language {
                Some(lang) => t.lang.to_string().to_lowercase() == lang.to_lowercase(),
                None => true,
            }
        }
    }) {
        None => {
            println!(
                "Template `{}` not found, select one of these templates:",
                cmd.template
            );

            //TODO: Use a TUI library or print in table format
            if !templates.is_empty() {
                println!("- Name, Lang");
                println!("===================");
            }
            for template in templates {
                println!("- {},  {}", template.name, template.lang);
            }
        }
        Some(template) => {
            let mut args = HashMap::new();
            args.insert("name".to_string(), cmd.name.clone());
            let path = cmd.path.unwrap_or(format!("./{}", cmd.name));
            let path = Path::new(&path);

            template.create(path, args)?;
        }
    }
    Ok(())
}

pub fn execute_build(_config: Config) -> Result<()> {
    template::MUManifest::read_file(None)?.build_project()
}

pub async fn execute_run(_config: Config) -> Result<()> {
    let manifest = template::MUManifest::read_file(None)?;
    manifest.build_project()?; //TODO: should we build on run or not?

    let stack = runtime::read_stack(None)?;

    let (runtime, gateway, database, gateways, stack_id) =
        runtime::start(stack, &manifest.wasm_module_path()).await?;

    let cancellation_token = CancellationToken::new();
    ctrlc::set_handler({
        let cancellation_token = cancellation_token.clone();
        move || {
            println!("Received SIGINT, stopping ...");
            cancellation_token.cancel()
        }
    })
    .context("Failed to initialize Ctrl+C handler")?;

    println!("Following endpoints are deployed:");
    for gateway in gateways {
        for (path, endpoints) in gateway.endpoints {
            for endpoint in endpoints {
                println!(
                    "- {}:{} : {} {}/{path}",
                    endpoint.route_to.assembly,
                    endpoint.route_to.function,
                    endpoint.method,
                    gateway.name
                );
            }
        }
    }

    println!("\nStack deployed at: http://localhost:12012/{stack_id}/");

    tokio::spawn({
        async move {
            loop {
                select! {
                    () = cancellation_token.cancelled() => {
                        [
                            runtime.stop().await.map_err(Into::into),
                            gateway.stop().await,
                            database.stop().await
                        ].into_iter().bcollect::<()>().unwrap();
                        break
                    }
                }
            }
        }
    })
    .await?;

    Ok(())
}
