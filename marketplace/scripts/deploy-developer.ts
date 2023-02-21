import { AnchorProvider } from "@project-serum/anchor";
import { createEscrowAccount, readOrCreateUserWallet, getMu, loadProviderFromStaticKeypair, readMintFromStaticKeypair, mintToAccount, getRegion, createApiRequestSigner, readOrCreateWallet } from "./anchor-utils";
import util from "./util"

util.asyncMain(async () => {
    let anchorProvider = AnchorProvider.local();

    let mint = readMintFromStaticKeypair();
    let mu = getMu(anchorProvider, mint);
    let provider = await loadProviderFromStaticKeypair(mu, "IB");
    let region = getRegion(mu, provider, 1);

    console.log("Creating developer and deploying escrow account");
    let userWallet = await readOrCreateUserWallet(mu, 1);
    let escrowAccount = await createEscrowAccount(mu, userWallet.keypair, provider);
    console.log(`Escrow PDA: ${escrowAccount.pda}`);
    await mintToAccount(anchorProvider, escrowAccount.pda, mint, 60_000000);
    let signer = await readOrCreateWallet(mu, "request-signer_1");
    await createApiRequestSigner(mu, userWallet.keypair, signer.keypair, region);
});
