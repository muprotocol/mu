use anyhow::{bail, Result};
use mu::mudb::{self, client::DatabaseID};
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

async fn install_wasm32_wasi_target() -> Result<()> {
    Command::new("rustup")
        .args(["target", "add", "wasm32-wasi"])
        .spawn()?
        .wait()
        .await?;
    Ok(())
}

pub async fn compile_wasm_project(project_dir: &Path) -> Result<PathBuf> {
    let project_dir = ensure_project_dir(project_dir)?;
    install_wasm32_wasi_target().await?;

    Command::new("cargo")
        .current_dir(&project_dir)
        .env_remove("CARGO_TARGET_DIR")
        .arg("build")
        .args(["--release", "--target", "wasm32-wasi"])
        .spawn()?
        .wait()
        .await?;

    Ok(project_dir.join("target/wasm32-wasi/release/"))
}

//TODO: maybe some `make clean` usage for this function
#[allow(dead_code)]
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

fn database_id_to_string(database_id: DatabaseID) -> String {
    format!(
        "{}_{}",
        database_id.stack_id,
        database_id.database_name.replace(' ', "-")
    )
}

pub async fn create_db(database_id: DatabaseID) -> Result<()> {
    let mut conf = mudb::Config::default();
    conf.name = database_id_to_string(database_id);
    tokio::task::spawn_blocking(|| mudb::MuDB::create_db(conf)).await??;
    Ok(())
}
