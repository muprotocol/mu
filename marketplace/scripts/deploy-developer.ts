import { AnchorProvider } from "@project-serum/anchor";
import { createEscrowAccount, readOrCreateUserWallet, getMu, loadProviderFromStaticKeypair, readMintFromStaticKeypair } from "./anchor-utils";
import util from "./util"

util.asyncMain(async () => {
    let anchorProvider = AnchorProvider.local();

    let mint = readMintFromStaticKeypair();
    let mu = getMu(anchorProvider, mint);
    let provider = await loadProviderFromStaticKeypair(mu, "IB");

    console.log("Creating developer and deploying escrow account");
    let userWallet = await readOrCreateUserWallet(mu, 1);
    await createEscrowAccount(mu, userWallet.keypair, provider);
});
