use std::{collections::HashMap, path::Path, str::FromStr};

use anyhow::{bail, Context, Result};
use clap::{Args, Parser};

use crate::{
    config::{Config, ConfigOverride},
    local_run,
    mu_manifest::{self, BuildMode, MUManifest},
    template::{Language, TemplateSet},
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
    Build(BuildCommand),

    /// Run mu project
    Run(RunCommand),
}

#[derive(Debug, Args)]
pub struct InitCommand {
    /// Initialize a new mu project.
    name: String,

    /// The directory to create new project in.
    path: Option<String>,

    #[arg(short, long)]
    /// Template to use for new project. Defaults to `empty` template
    template: Option<String>,

    #[arg(short, long)]
    /// Language. Defaults to Rust
    language: Option<String>,
}

#[derive(Debug, Args)]
pub struct BuildCommand {
    #[arg(long)]
    /// Build artifacts in release mode, with optimizations
    release: bool,
}

#[derive(Debug, Args)]
pub struct RunCommand {
    #[arg(long)]
    /// Build artifacts in release mode, with optimizations
    release: bool,
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
        Command::Build(sub_command) => execute_build(config, sub_command),
        Command::Run(sub_command) => execute_run(config, sub_command).await,
    }
}

pub fn execute_init(_config: Config, cmd: InitCommand) -> Result<()> {
    let template_sets =
        TemplateSet::load_builtins().context("Can not deserialize builtin template sets")?;

    let lang = cmd
        .language
        .map(|s| Language::from_str(&s))
        .unwrap_or(Ok(Language::Rust))
        .context("Invalid language")?;
    let template_name = cmd.template.unwrap_or("empty".to_string());

    if let Some(template_set) = template_sets.iter().find(|t| t.name == template_name) {
        let mut args = HashMap::new();
        args.insert("name".to_string(), cmd.name.clone());
        let path = cmd.path.unwrap_or(format!("./{}", cmd.name));
        let path = Path::new(&path);

        if path
            .try_exists()
            .context("Failed to check the destination directory")?
        {
            bail!("Destination `{}` already exists", path.display());
        }

        template_set.create(lang, path, args)?;
        println!("Created new project ({}) `{}`", template_set.name, cmd.name);
    } else {
        println!("Template `{template_name}` not found");
        TemplateSet::print_all(&template_sets);
    }
    Ok(())
}

pub fn execute_build(_config: Config, cmd: BuildCommand) -> Result<()> {
    let build_mode = if cmd.release {
        BuildMode::Release
    } else {
        BuildMode::Debug
    };

    read_manifest()?.build_project(build_mode)
}

pub async fn execute_run(_config: Config, cmd: RunCommand) -> Result<()> {
    let manifest = read_manifest()?;

    let build_mode = if cmd.release {
        BuildMode::Release
    } else {
        BuildMode::Debug
    };

    manifest.build_project(build_mode)?;

    let stack = manifest
        .generate_stack_manifest(mu_manifest::ArtifactGenerationMode::LocalRun)
        .context("failed to generate stack.")?;

    local_run::start_local_node((stack, manifest.test_id)).await
}

fn read_manifest() -> Result<MUManifest> {
    let path = std::env::current_dir()?.join(mu_manifest::MU_MANIFEST_FILE_NAME);

    if !path.try_exists()? {
        bail!(
            "Not in a mu project, `{}` file not found.",
            mu_manifest::MU_MANIFEST_FILE_NAME
        );
    }

    let mut file = std::fs::File::open(path)?;
    mu_manifest::MUManifest::read(&mut file)
}
