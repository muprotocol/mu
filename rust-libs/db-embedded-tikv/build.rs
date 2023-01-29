use std::{fs, path::Path};

use anyhow::Context;
use flate2::bufread::GzDecoder;
use tar::Archive;

const TIKV_VERSION: &str = "6.4.0";

fn download_and_extract_file(url: String, dest: &str, file_name: &str) {
    let new_path = Path::new(dest).join(format!("{file_name}-{TIKV_VERSION}"));

    if new_path.exists() {
        return;
    }
    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(3600))
        .build()
        .unwrap();

    let req = client.get(url);
    let bytes = req
        .send()
        .context("Failed to send request")
        .unwrap()
        .bytes()
        .context("Failed to get bytes")
        .unwrap();

    let file = GzDecoder::new(&bytes[..]);
    let mut archive = Archive::new(file);
    archive
        .unpack(dest)
        .context(format!("Failed to extract file:{file_name}"))
        .unwrap();

    let path = Path::new(dest).join(file_name);

    fs::rename(path, new_path)
        .context("Failed to rename file")
        .unwrap();
}

fn main() {
    println!("cargo:rustc-env=TIKV_VERSION={TIKV_VERSION}");

    let pd_url = format!("https://tiup-mirrors.pingcap.com/pd-v{TIKV_VERSION}-linux-amd64.tar.gz");
    let tikv_url =
        format!("https://tiup-mirrors.pingcap.com/tikv-v{TIKV_VERSION}-linux-amd64.tar.gz");

    download_and_extract_file(pd_url, "assets", "pd-server");
    download_and_extract_file(tikv_url, "assets", "tikv-server");
}
