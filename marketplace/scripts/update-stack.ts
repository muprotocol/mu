import util from './util';
import path from 'path'
import * as stackUtil from './stack-util'
import { AnchorProvider } from '@project-serum/anchor';
import { getMu, getRegion, loadProviderFromStaticKeypair, readMintFromStaticKeypair, readOrCreateUserWallet, updateStack } from './anchor-utils';

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

    console.log("Updating stack");

    let protoBytes = stackUtil.yamlToProto(path.resolve(__dirname, "test-stack/stack-v2.yaml"));
    await updateStack(mu, userWallet.keypair, region, Buffer.from(protoBytes), stackSeed, "test stack v2");
});