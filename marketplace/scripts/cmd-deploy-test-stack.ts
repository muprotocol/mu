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

    console.log("Starting local HTTP server to serve function code");
    muxer.spawnNew(`npx ts-node ${path.resolve(__dirname, "start-local-http-server.ts")}`, "http-server");

    console.log("Deploying Mu smart contract");
    muxer.spawnNew(
        `export BROWSER='' ANCHOR_WALLET='${await initializeAndGetAuthorityWalletPath()}' && ` +
        `cd '${process.cwd()}' && ` +
        `env -C ${path.resolve(__dirname, "..")} anchor build && ` +
        `npx ts-node ${path.resolve(__dirname, "deploy-contract.ts")} && ` +
        `npx ts-node ${path.resolve(__dirname, "initialize-mu.ts")} && ` +
        `npx ts-node ${path.resolve(__dirname, "deploy-provider.ts")} && ` +
        `npx ts-node ${path.resolve(__dirname, "deploy-developer.ts")}`,
        "deploy"
    );

    await muxer.waitForAllWithSigint();
})
