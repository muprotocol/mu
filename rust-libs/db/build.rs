use std::process;

// The abseil-cpp library refuses to build, so we have to patch its source.
// If and when we update tikv_client to a more recent version, this should
// no longer be necessary and can be removed.
// This is a hack in every sense of the word; it depends on cargo running
// the build in parallel, and hopes it gets to run before the source file
// has a chance to be compiled.
fn patch_abseil_source() {
    let mut path = dirs::home_dir().unwrap();
    path.push(".cargo/registry/src/github.com-1ecc6299db9ec823/grpcio-sys-0.8.1/grpc/third_party/abseil-cpp/absl/synchronization/internal/graphcycles.cc");

    let status = process::Command::new("grep")
        .arg("-q")
        .arg("#include <limits>")
        .arg(&path)
        .status()
        .expect("Failed to check abseil source files");

    if status.success() {
        return;
    }

    let status = process::Command::new("sed")
        .arg("-i")
        .arg(r#":a;N;$!ba;s/#include <array>\n#include "absl/#include <array>\n#include <limits>\n#include "absl/g"#)
        .arg(&path)
        .status()
        .expect("Failed to patch abseil source files");
    assert!(status.success());
}

fn main() {
    patch_abseil_source();
}
