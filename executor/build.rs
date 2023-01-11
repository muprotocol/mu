use std::{env, path::PathBuf};

fn main() {
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
}
