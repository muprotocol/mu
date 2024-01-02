use std::path::Path;

use anyhow::Context;
use flate2::bufread::GzDecoder;
use tar::Archive;

const JUICEFS_VERSION: &str = "1.0.3";

fn download_and_extract_file(url: String, dest_folder: &str, file_name: &str) {
    let new_path = Path::new(dest_folder).join(format!("{file_name}-{JUICEFS_VERSION}"));

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

    std::fs::create_dir_all(dest_folder).unwrap();

    let file = GzDecoder::new(&bytes[..]);
    let mut archive = Archive::new(file);
    for entry in archive.entries().unwrap() {
        let mut entry = entry.unwrap();
        if entry.path().unwrap().starts_with("juicefs") {
            entry
                .unpack(new_path)
                .context("Failed to extract juicefs binary")
                .unwrap();
            break;
        }
    }
}

fn main() {
    println!("cargo:rustc-env=TAG_NAME={JUICEFS_VERSION}");

    let juicefs_url = format!("https://github.com/juicedata/juicefs/releases/download/v{JUICEFS_VERSION}/juicefs-{JUICEFS_VERSION}-linux-amd64.tar.gz");
    download_and_extract_file(juicefs_url, "assets", "juicefs")
}
