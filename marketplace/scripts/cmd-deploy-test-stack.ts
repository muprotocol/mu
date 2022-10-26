import { existsSync } from "fs";
import path from "path";
import { TmuxSession } from "./tmux";
import util from "./util"
import promptSync from "prompt-sync";
import { canConnectToLocalValidator } from "./anchor-utils";
import { env } from "process";
import { homedir } from "os";

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
    // TODO: swap tmux out for running the processes as children directly
    let tmuxSession = new TmuxSession(sessionName, "solana-test-validator");

    console.log("Waiting for validator to start");
    util.waitUntilPortUsed(8899);
    env.ANCHOR_WALLET = path.resolve(homedir(), ".config/solana/id.json");
    while (!(await canConnectToLocalValidator())) {
        await util.sleep(0.5);
    }

    console.log("Starting local HTTP server to serve function code");
    tmuxSession.splitWindow(`npx ts-node ${path.resolve(__dirname, "start-local-http-server.ts")}`, 0, true);

    console.log("Deploying Mu smart contract");
    tmuxSession.splitWindow(
        `export BROWSER='' ANCHOR_WALLET='~/.config/solana/id.json' && ` +
        `cd '${process.cwd()}' && ` +
        `npx ts-node ${path.resolve(__dirname, "deploy-contract.ts")} && ` +
        `npx ts-node ${path.resolve(__dirname, "deploy-mint-and-provider.ts")} && ` +
        `npx ts-node ${path.resolve(__dirname, "deploy-developer.ts")} && ` +
        `npx ts-node ${path.resolve(__dirname, "deploy-stack.ts")} && ` +
        `sleep 10`,
        0,
        true);

    tmuxSession.attach();
})
