import util from './util';
import path from 'path'
import * as stackUtil from './stack-util'
import { AnchorProvider } from '@project-serum/anchor';
import { deployStack, getMu, getRegion, loadProviderFromStaticKeypair, readMintFromStaticKeypair, readOrCreateUserWallet } from './anchor-utils';

util.asyncMain(async () => {
    let stackSeed = parseInt(process.argv[2]);
    if (stackSeed == NaN)
        stackSeed = 1;

    let anchorProvider = AnchorProvider.local();
    let mint = readMintFromStaticKeypair();

    let mu = getMu(anchorProvider, mint);

    let provider = await loadProviderFromStaticKeypair(mu, "IB");
    let region = getRegion(mu, provider, 1);

    let userWallet = await readOrCreateUserWallet(mu, 1);

    console.log("Dploying stack");

    let protoBytes = stackUtil.yamlToProto(path.resolve(__dirname, "test-stack/stack.yaml"));
    let stack = await deployStack(mu, userWallet.keypair, region, Buffer.from(protoBytes), stackSeed);

    console.log("Stack key:", stack.pda);
});