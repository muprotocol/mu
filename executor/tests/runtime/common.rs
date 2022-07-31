use anyhow::{bail, Result};
use std::{
    env,
    path::{Path, PathBuf},
};
use tokio::process::Command;

fn ensure_project_dir(project_dir: &Path) -> Result<PathBuf> {
    let project_dir = env::current_dir()?.join(project_dir);
    if !project_dir.is_dir() {
        bail!("project dir should be a directory")
    }
    Ok(project_dir)
}

pub async fn compile_wasm_project(project_dir: &Path) -> Result<PathBuf> {
    let project_dir = ensure_project_dir(project_dir)?;

    Command::new("cargo")
        .current_dir(&project_dir)
        .env_remove("CARGO_TARGET_DIR")
        .arg("build")
        .args(["--release", "--target", "wasm32-wasi"])
        .spawn()?
        .wait()
        .await?;

    Ok(project_dir
        .join("target/wasm32-wasi/release/")
        .to_path_buf())
}

pub async fn clean_wasm_project(project_dir: &Path) -> Result<()> {
    let project_dir = ensure_project_dir(project_dir)?;

    Command::new("cargo")
        .current_dir(&project_dir)
        .env_remove("CARGO_TARGET_DIR")
        .arg("clean")
        .spawn()?
        .wait()
        .await?;

    Ok(())
}
