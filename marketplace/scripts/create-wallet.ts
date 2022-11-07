import { readOrCreateKeypair } from "./anchor-utils";

let walletName = process.argv[2];
readOrCreateKeypair(walletName);
