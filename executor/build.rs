fn main() {
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
