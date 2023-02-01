import util from './util';
import path from 'path'
import { AnchorProvider } from '@project-serum/anchor';
import { deleteStack, getMu, getRegion, loadProviderFromStaticKeypair, readMintFromStaticKeypair, readOrCreateUserWallet } from './anchor-utils';

util.asyncMain(async () => {
    let stackSeed = parseInt(process.argv[2]);
    if (Number.isNaN(stackSeed))
        stackSeed = 1;

    let anchorProvider = AnchorProvider.local();
    let mint = readMintFromStaticKeypair();

    let mu = getMu(anchorProvider, mint);

    let provider = await loadProviderFromStaticKeypair(mu, "IB");
    let region = getRegion(mu, provider, 1);

    let userWallet = await readOrCreateUserWallet(mu, 1);

    console.log("Deleting stack");

    await deleteStack(mu, userWallet.keypair, region, stackSeed);
});