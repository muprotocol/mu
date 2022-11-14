import path from "path";
import util from "./util"
import {
    getDefaultWalletPath,
    getSolanaValidatorCommand,
    promptForRemovalIfLedgerExists,
    waitForLocalValidatorToStart
} from "./anchor-utils";
import {ProcessMultiplexer} from "./process-multiplexer";

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
        `export BROWSER='' ANCHOR_WALLET='${getDefaultWalletPath()}' && ` +
        `cd '${process.cwd()}' && ` +
        `npx ts-node ${path.resolve(__dirname, "deploy-contract.ts")} && ` +
        `npx ts-node ${path.resolve(__dirname, "initialize-mu.ts")} && ` +
        `npx ts-node ${path.resolve(__dirname, "deploy-provider.ts")} && ` +
        `npx ts-node ${path.resolve(__dirname, "deploy-developer.ts")} && ` +
        `npx ts-node ${path.resolve(__dirname, "deploy-stack.ts")} && ` +
        `sleep 10`,
        "deploy"
    );

    await muxer.waitForAllWithSigint();
})
