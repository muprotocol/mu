
import { AnchorProvider } from "@project-serum/anchor";
import { argv } from "process";
import { authorizeProvider, getMu, loadProviderFromKeypair, readKeypair, readMintFromStaticKeypair, readProviderAuthorizer } from "./anchor-utils";
import util from "./util"

util.asyncMain(async () => {
    let keypairPath = argv[2];
    if (!keypairPath) {
        throw new Error("Received empty keypair path");
    }
    let keypair = readKeypair(keypairPath);

    let anchorProvider = AnchorProvider.local();
    let mint = readMintFromStaticKeypair();
    let mu = getMu(anchorProvider, mint);

    let provider = await loadProviderFromKeypair(mu, keypair);

    console.log("Authorizing provider");
    let authorizer = readProviderAuthorizer(mu, "1");
    await authorizeProvider(mu, provider, authorizer);
});