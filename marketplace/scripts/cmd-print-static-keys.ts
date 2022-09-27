import * as anchor from "@project-serum/anchor";
import * as fs from "fs";
import path from "path";

let basePath = path.join(__dirname, "test-wallets");
for (let name of fs.readdirSync(basePath)) {
    if (name == "README.md")
        continue;

    let bytes: Uint8Array = fs.readFileSync(path.join(basePath, name));
    let keypair = anchor.web3.Keypair.fromSecretKey(bytes);
    console.log(`${name}: ${keypair.publicKey.toBase58()}`);
}