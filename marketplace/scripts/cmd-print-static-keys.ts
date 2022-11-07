import * as anchor from "@project-serum/anchor";
import { bs58 } from "@project-serum/anchor/dist/cjs/utils/bytes";
import * as fs from "fs";
import path from "path";

let basePath = path.join(__dirname, "test-wallets");
for (let name of fs.readdirSync(basePath)) {
    if (name == "README.md")
        continue;

    let bytes: Uint8Array = fs.readFileSync(path.join(basePath, name));
    let keypair = anchor.web3.Keypair.fromSecretKey(bytes);
    console.log(`${name} (Public): ${keypair.publicKey.toBase58()}`);
    console.log(`${name} (Private): ${bs58.encode(keypair.secretKey)}`);
}