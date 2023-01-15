use anyhow::{Context, Result};
use flate2::bufread::GzDecoder;
use std::{env, path::PathBuf};
use std::{fs::rename, path::Path};
use tar::Archive;

const TIKV_VERSION: &str = "6.4.0";

fn download_and_extract_file(url: String, dest: &str, file_name: &str) -> Result<()> {
    let new_path = Path::new(dest).join(format!("{file_name}-{TIKV_VERSION}"));

    // TODO: figure out whats wrong with rerun-if-changed then remove this
    if new_path.exists() {
        return Ok(());
    }
    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(3600))
        .build()
        .unwrap();

    let req = client.get(url);
    let bytes = req
        .send()
        .context("Failed to send request")?
        .bytes()
        .context("Failed to get bytes")?;

    let file = GzDecoder::new(&bytes[..]);
    let mut archive = Archive::new(file);
    archive
        .unpack(dest)
        .context(format!("Failed to extract file:{file_name}"))?;

    let path = Path::new(dest).join(file_name);

    rename(path, new_path).context("Failed to rename file")?;

    Ok(())
}

fn main() {
    println!("cargo:rerun-if-changed=assets/pd-server-6.4.0");
    println!("cargo:rustc-env=TIKV_VERSION={TIKV_VERSION}");
    let pd_url = format!("https://tiup-mirrors.pingcap.com/pd-v{TIKV_VERSION}-linux-amd64.tar.gz");
    let tikv_url =
        format!("https://tiup-mirrors.pingcap.com/tikv-v{TIKV_VERSION}-linux-amd64.tar.gz");

    download_and_extract_file(pd_url, "assets", "pd-server").unwrap();
    download_and_extract_file(tikv_url, "assets", "tikv-server").unwrap();
    let cargo_out_dir = env::var("OUT_DIR").expect("OUT_DIR env var not set");
    let mut path = PathBuf::from(cargo_out_dir);
    path.push("protos");
    path.push("rpc");
    std::fs::create_dir_all(&path).unwrap();
    path.pop();
    path.push("gossip");
    std::fs::create_dir_all(&path).unwrap();

    protobuf_codegen::Codegen::new()
        .protoc()
        .protoc_path(&protoc_bin_vendored::protoc_bin_path().unwrap())
        .includes(["protos", "../mu_stack/protos"])
        .input("protos/rpc.proto")
        .cargo_out_dir("protos/rpc")
        .run_from_script();
    protobuf_codegen::Codegen::new()
        .protoc()
        .protoc_path(&protoc_bin_vendored::protoc_bin_path().unwrap())
        .includes(["protos"])
        .input("protos/gossip.proto")
        .cargo_out_dir("protos/gossip")
        .run_from_script();

    println!("build script ran");
}
