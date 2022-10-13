import { AnchorProvider } from "@project-serum/anchor";
import { createMint, initializeMu } from "./anchor-utils";
import util from "./util"

util.asyncMain(async () => {
    let anchorProvider = AnchorProvider.local("http://127.0.0.1:8899");

    console.log("Deploying mint");
    let mint = await createMint(anchorProvider, true);

    console.log("Initializing Mu smart contract");
    await initializeMu(anchorProvider, mint);
});
