use anyhow::{Context, Result};
use flate2::bufread::GzDecoder;
use std::{fs, path::Path, process};
use tar::Archive;

const TIKV_VERSION: &str = "6.4.0";

// The abseil-cpp library refuses to build, so we have to patch its source.
// If and when we update tikv_client to a more recent version, this should
// no longer be necessary and can be removed.
// This is a hack in every sense of the word; it depends on cargo running
// the build in parallel, and hopes it gets to run before the source file
// has a chance to be compiled.
fn patch_abseil_source() {
    let mut path = dirs::home_dir().unwrap();
    path.push(".cargo/registry/src/github.com-1ecc6299db9ec823/grpcio-sys-0.8.1/grpc/third_party/abseil-cpp/absl/synchronization/internal/graphcycles.cc");
    println!("{}", path.to_str().unwrap());
    let status = process::Command::new("sed")
        .arg("-i")
        .arg(r#":a;N;$!ba;s/#include <array>\n#include "absl/#include <array>\n#include <limits>\n#include "absl/g"#)
        .arg(path)
        .status()
        .expect("Failed to patch abseil source files");
    assert!(status.success());
}

fn download_and_extract_file(url: String, dest: &str, file_name: &str) -> Result<()> {
    let new_path = Path::new(dest).join(format!("{file_name}-{TIKV_VERSION}"));

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

    fs::rename(path, new_path).context("Failed to rename file")?;

    Ok(())
}

fn main() {
    patch_abseil_source();

    println!("cargo:rerun-if-changed=assets");
    println!("cargo:rerun-if-changed=protos");
    println!("cargo:rerun-if-changed=build.rs");

    println!("cargo:rustc-env=TIKV_VERSION={TIKV_VERSION}");

    let pd_url = format!("https://tiup-mirrors.pingcap.com/pd-v{TIKV_VERSION}-linux-amd64.tar.gz");
    let tikv_url =
        format!("https://tiup-mirrors.pingcap.com/tikv-v{TIKV_VERSION}-linux-amd64.tar.gz");

    download_and_extract_file(pd_url, "assets", "pd-server").unwrap();
    download_and_extract_file(tikv_url, "assets", "tikv-server").unwrap();
}
