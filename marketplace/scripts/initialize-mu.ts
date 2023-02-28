import { AnchorProvider } from "@project-serum/anchor";
import { BN } from "bn.js";
import { createMint, createProviderAuthorizer, initializeMu } from "./anchor-utils";
import util from "./util";

util.asyncMain(async () => {
    let anchorProvider = AnchorProvider.local();

    console.log("Deploying mint");
    let mint = await createMint(anchorProvider, true);

    console.log("Initializing Mu smart contract");
    let mu = await initializeMu(anchorProvider, mint, 100_000, new BN(200_000000));

    console.log("Creating provider authorizer");
    await createProviderAuthorizer(mu, "1");
});