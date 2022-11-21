import { AnchorProvider } from "@project-serum/anchor";
import {
    readOrCreateUserWallet,
    getMu,
    loadProviderFromStaticKeypair,
    readMintFromStaticKeypair,
    getEscrowAccount, mintToAccount
} from "./anchor-utils";
import util from "./util"

util.asyncMain(async () => {
    let anchorProvider = AnchorProvider.local();

    let mint = readMintFromStaticKeypair();
    let mu = getMu(anchorProvider, mint);
    let provider = await loadProviderFromStaticKeypair(mu, "IB");
    let userWallet = await readOrCreateUserWallet(mu, 1);
    let escrow = await getEscrowAccount(mu, userWallet.keypair, provider);

    await mintToAccount(anchorProvider, escrow.pda, mu.mint, 10_000000);
    let escrowBalance = await anchorProvider.connection.getTokenAccountBalance(escrow.pda, 'processed');
    console.log(`Done, final escrow balance is: ${escrowBalance.value.uiAmountString}`);
});