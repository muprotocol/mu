import path from "path";
import { run } from "./util";

run("rustup target add wasm32-wasi");
run(`cd ${path.resolve(__dirname, "test-stack", "test-function")} && ` +
    `cargo build --release --target wasm32-wasi && ` +
    `cp ./target/wasm32-wasi/release/test-function.wasm ..`);
run(`cd ${path.resolve(__dirname, "test-stack")} && npx http-server`);
