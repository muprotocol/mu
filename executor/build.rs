use anyhow::{Context, Result};
use flate2::bufread::GzDecoder;
use std::{fs::rename, path::Path};
use tar::Archive;

const TIKV_VERSION: &str = "6.4.0";

fn download_and_extract_file(url: String, dest: &str, file_name: &str) -> Result<()> {
    let client = reqwest::blocking::Client::new();
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
    let new_path = Path::new(dest).join(format!("{file_name}-{TIKV_VERSION}"));

    rename(path, new_path).context("Failed to rename file")?;

    Ok(())
}

fn main() {
    // println!("cargo:rerun-if-changed=assets");
    // println!("cargo:rustc-env=TIKV_VERSION={TIKV_VERSION}");

    // // let pd_url = format!("https://tiup-mirrors.pingcap.com/pd-v{TIKV_VERSION}-linux-amd64.tar.gz");
    // let pd_url = format!("http://0.0.0.0:8080/pd-v{TIKV_VERSION}-linux-amd64.tar.gz");
    // let tikv_url = format!("http://0.0.0.0:8080/tikv-v{TIKV_VERSION}-linux-amd64.tar.gz");
    // // format!("https://tiup-mirrors.pingcap.com/tikv-v{TIKV_VERSION}-linux-amd64.tar.gz");

    // download_and_extract_file(pd_url, "assets", "pd-server").unwrap();
    // download_and_extract_file(tikv_url, "assets", "tikv-server").unwrap();
}
