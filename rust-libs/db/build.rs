use std::{path::Path, process};

fn patch_file(path: &Path) {
    let status = process::Command::new("grep")
        .arg("-q")
        .arg("#include <limits>")
        .arg(&path)
        .status()
        .expect("Failed to check abseil source files");

    if status.success() {
        return;
    }

    process::Command::new("sed")
        .arg("-i")
        .arg(r#":a;N;$!ba;s/#include <array>\n#include "absl/#include <array>\n#include <limits>\n#include "absl/g"#)
        .arg(&path)
        .status()
        .expect("Failed to patch abseil source files");
}

// The abseil-cpp library refuses to build, so we have to patch its source.
// If and when we update tikv_client to a more recent version, this should
// no longer be necessary and can be removed.
// This is a hack in every sense of the word; it depends on cargo running
// the build in parallel, and hopes it gets to run before the source file
// has a chance to be compiled.
fn patch_abseil_source() {
    let home = dirs::home_dir().unwrap();

    let path1 = home.join(".cargo/registry/src/index.crates.io-6f17d22bba15001f/grpcio-sys-0.8.1/grpc/third_party/abseil-cpp/absl/synchronization/internal/graphcycles.cc");
    let path2 = home.join(".cargo/registry/src/github.com-1ecc6299db9ec823/grpcio-sys-0.8.1/grpc/third_party/abseil-cpp/absl/synchronization/internal/graphcycles.cc");

    patch_file(&path1);
    patch_file(&path2);
}

fn main() {
    patch_abseil_source();
}
