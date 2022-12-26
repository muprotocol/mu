use anyhow::{Context, Result};
use flate2::bufread::GzDecoder;
use std::io::BufReader;
use tar::Archive;

fn download_and_extract_file(url: String, dest: &str) -> Result<()> {
    let resp = reqwest::blocking::get(&url).unwrap();
    let content_br = BufReader::new(resp);
    let file = GzDecoder::new(content_br);
    let mut archive = Archive::new(file);
    archive
        .unpack(dest)
        .context(format!("Failed to extract file:{dest}"))
        .unwrap();

    Ok(())
}

fn main() {
    println!("cargo:rerun-if-changed=assets");
    println!("cargo:rustc-env=TIKV_VERSION=6.0.4");

    let tikv_version = env!("TIKV_VERSION");

    let pd_url = format!("https://tiup-mirrors.pingcap.com/pd-v{tikv_version}-linux-amd64.tar.gz");
    let tikv_url =
        format!("https://tiup-mirrors.pingcap.com/tikv-v{tikv_version}-linux-amd64.tar.gz");

    download_and_extract_file(pd_url, "assets").unwrap();
    download_and_extract_file(tikv_url, "assets").unwrap();
}
