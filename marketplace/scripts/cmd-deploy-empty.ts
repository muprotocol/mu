import { existsSync } from "fs";
import path from "path";
import promptSync from "prompt-sync";
import { TmuxSession } from "./tmux";
import util from "./util"

util.asyncMain(async () => {
    if (existsSync("test-ledger")) {
        let prompt = promptSync();
        if (!process.argv.includes("-y") &&
            prompt("This command will delete the Solana ledger in ./test-ledger, are you sure? [y/n] ") != "y")
            return;

        util.run("rm -rf test-ledger");
    }

    console.log("Building anchor project");
    util.run("anchor build");

    let sessionName = `mu_marketplace_${Date.now()}`;
    console.log(`Starting tmux session ${sessionName}`);
    console.log("Starting local Solana validator");
    let tmuxSession = new TmuxSession(sessionName, "solana-test-validator");

    console.log("Waiting for validator to start");
    util.waitUntilPortUsed(8899);
    // Wait an additional 2 seconds for the node to become healthy
    await util.sleep(2);

    console.log("Deploying Mu smart contract");
    tmuxSession.splitWindow(
        `export BROWSER='' ANCHOR_WALLET='~/.config/solana/id.json' && ` +
        `cd ${process.cwd()} && ` +
        `npx ts-node ${path.resolve(__dirname, "deploy-contract.ts")} && ` +
        `npx ts-node ${path.resolve(__dirname, "initialize-mu.ts")} && ` +
        `echo Done && ` +
        `sleep 5`,
        0,
        true);

    tmuxSession.attach();
})
