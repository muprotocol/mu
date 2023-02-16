use std::{borrow::Cow, collections::HashMap, fs, path::Path, process::exit, str::FromStr};

use anyhow::{bail, Context, Result};
use clap::Args;

use crate::{
    local_run,
    mu_manifest::{self, BuildMode, MUManifest},
    template::{Language, TemplateSet},
};

#[derive(Debug, Args)]
pub struct InitCommand {
    /// The directory to create new project in
    path: Option<String>,

    /// The name of the project. Will use the name of the parent directory if left out
    name: Option<String>,

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

pub fn execute_init(cmd: InitCommand) -> Result<()> {
    let template_sets =
        TemplateSet::load_builtin().context("Can not deserialize builtin template sets")?;

    let lang = cmd
        .language
        .map(|s| Language::from_str(&s))
        .unwrap_or(Ok(Language::Rust))
        .context("Invalid language")?;
    let template_name = cmd.template.unwrap_or("empty".to_string());

    let Some(template_set) = template_sets.iter().find(|t| t.name == template_name) else {
        println!("Template `{template_name}` not found");
        TemplateSet::print_all(&template_sets);
        exit(-1);
    };

    let mut path = Path::new(
        cmd.path
            .map(Cow::Owned)
            .unwrap_or(Cow::Borrowed("."))
            .as_ref(),
    )
    .canonicalize()
    .context("Failed to get full path to destination directory")?;

    let name = match cmd.name {
        Some(n) => {
            path.push(&n);
            if path
                .try_exists()
                .context("Failed to check destination directory")?
            {
                bail!("Destination `{}` already exists", path.display());
            }
            n
        }
        None => {
            if !path
                .try_exists()
                .context("Failed to check destination directory")?
            {
                fs::create_dir(&path).context("Failed to create destination directory")?;
            }

            if fs::read_dir(&path)
                .context("Failed to read destination directory")?
                .next()
                .is_some()
            {
                bail!("Destination directory is not empty");
            }

            path.components()
                .last()
                .context("Empty path")?
                .as_os_str()
                .to_string_lossy()
                .into_owned()
        }
    };

    let mut args = HashMap::new();
    args.insert("name".to_string(), name.clone());

    template_set.create(lang, &path, args)?;
    println!("Created new project ({}) `{}`", template_set.name, name);

    Ok(())
}

pub fn execute_build(cmd: BuildCommand) -> Result<()> {
    let build_mode = if cmd.release {
        BuildMode::Release
    } else {
        BuildMode::Debug
    };

    read_manifest()?.build_project(build_mode)
}

pub fn execute_run(cmd: RunCommand) -> Result<()> {
    let manifest = read_manifest()?;

    let build_mode = if cmd.release {
        BuildMode::Release
    } else {
        BuildMode::Debug
    };

    manifest.build_project(build_mode)?;

    let stack = manifest
        .generate_stack_manifest(build_mode, mu_manifest::ArtifactGenerationMode::LocalRun)
        .context("failed to generate stack definition")?;

    tokio::runtime::Runtime::new()?.block_on(local_run::start_local_node((stack, manifest.dev_id)))
}

fn read_manifest() -> Result<MUManifest> {
    let mut path = std::env::current_dir()?;

    loop {
        let manifest_path = path.join(mu_manifest::MU_MANIFEST_FILE_NAME);
        if manifest_path.try_exists()? {
            let mut file = std::fs::File::open(manifest_path)?;
            return mu_manifest::MUManifest::read(&mut file);
        }
        let Some(parent) = path.parent() else {
            break
        };
        path = parent.into();
    }

    bail!(
        "Not in a mu project, `{}` file not found.",
        mu_manifest::MU_MANIFEST_FILE_NAME
    );
}
