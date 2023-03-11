use std::{borrow::Cow, collections::HashMap, fs, path::Path, process::exit, str::FromStr};

use anyhow::{bail, Context, Result};
use clap::Args;

use crate::{
    local_run,
    mu_manifest::{read_manifest, BuildMode},
    template::{Language, TemplateSet},
};

#[derive(Debug, Args)]
pub struct InitCommand {
    /// The directory to create new project in
    path: Option<String>,

    /// The name of the project. Will use the name of the parent directory if left out
    #[arg(short, long)]
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
        print_template_sets(&template_sets);
        exit(-1);
    };

    let mut path = Path::new(
        cmd.path
            .map(Cow::Owned)
            .unwrap_or(Cow::Borrowed("."))
            .as_ref(),
    )
    .to_path_buf();

    fs::create_dir_all(&path).context("Failed to create destination directory")?;

    path = path
        .canonicalize()
        .context("Failed to get full path to destination directory")?;

    if fs::read_dir(&path)
        .context("Failed to read destination directory")?
        .next()
        .is_some()
    {
        bail!("Destination directory is not empty");
    }

    let name = match cmd.name {
        Some(n) => n,
        None => path
            .components()
            .last()
            .context("Empty path")?
            .as_os_str()
            .to_string_lossy()
            .into_owned(),
    };

    let mut args = HashMap::new();
    args.insert("name".to_string(), name.clone());

    template_set.create(lang, &path, args)?;
    println!("Created new project ({}) `{}`", template_set.name, name);

    Ok(())
}

pub fn print_template_sets(sets: &[TemplateSet]) {
    println!("Available templates:\n");
    println!("{: ^10}|{: ^15}", "Name", "Languages");
    println!("{}", "-".repeat(26));

    sets.iter().for_each(|t| println!("{t}"));
}

pub fn execute_build(cmd: BuildCommand) -> Result<()> {
    let build_mode = if cmd.release {
        BuildMode::Release
    } else {
        BuildMode::Debug
    };

    let (manifest, project_root) = read_manifest()?;
    manifest.build_all(build_mode, &project_root)
}

pub fn execute_run(cmd: RunCommand) -> Result<()> {
    let (manifest, project_root) = read_manifest()?;

    let build_mode = if cmd.release {
        BuildMode::Release
    } else {
        BuildMode::Debug
    };

    manifest.build_all(build_mode, &project_root)?;

    let stack = manifest
        .generate_stack_manifest_for_local_run(build_mode)
        .context("failed to generate stack definition")?;

    tokio::runtime::Runtime::new()?.block_on(local_run::start_local_node(
        (stack, manifest.dev_id),
        project_root,
    ))
}
