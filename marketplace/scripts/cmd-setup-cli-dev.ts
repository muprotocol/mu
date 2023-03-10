// Set up a validator for developing the CLI:
// * Deploy the smart contract
// * Create and fund a provider wallet
// * Create and fund a developer wallet

import path from "path";
import util from "./util"
import {
    getSolanaValidatorCommand,
    initializeAndGetAuthorityWalletPath,
    promptForRemovalIfLedgerExists,
    waitForLocalValidatorToStart
} from "./anchor-utils";
import { ProcessMultiplexer } from "./process-multiplexer";

util.asyncMain(async () => {
    if (!promptForRemovalIfLedgerExists()) {
        return;
    }

    console.log("Building anchor project");
    util.run("anchor build");

    let muxer = new ProcessMultiplexer();
    muxer.spawnNew(getSolanaValidatorCommand(), "solana");

    await waitForLocalValidatorToStart();

    console.log("Deploying Mu smart contract");
    muxer.spawnNew(
        `export BROWSER='' ANCHOR_WALLET='${await initializeAndGetAuthorityWalletPath()}' && ` +
        `cd '${process.cwd()}' && ` +
        `env -C ${path.resolve(__dirname, "..")} anchor build && ` +
        `npx ts-node ${path.resolve(__dirname, "deploy-contract.ts")} && ` +
        `npx ts-node ${path.resolve(__dirname, "initialize-mu.ts")} && ` +
        `npx ts-node ${path.resolve(__dirname, "fund-wallet.ts")} cli_provider && ` +
        `npx ts-node ${path.resolve(__dirname, "fund-wallet.ts")} cli_dev && ` +
        `npx ts-node ${path.resolve(__dirname, "create-wallet.ts")} cli_signer && ` +
        `echo Done`,
        "deploy"
    );

    await muxer.waitForAllWithSigint();
})
