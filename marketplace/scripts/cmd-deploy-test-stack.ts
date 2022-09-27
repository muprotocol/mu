import path from "path";
import { TmuxSession } from "./tmux";
import util from "./util"

util.asyncMain(async () => {
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
        `env -C '${process.cwd()}' BROWSER='' ANCHOR_WALLET='/home/vscode/.config/solana/id.json' ` +
        `npx ts-node ${path.resolve(__dirname, "deploy-contract.ts")} && ` +
        `npx ts-node ${path.resolve(__dirname, "deploy-mint-and-provider.ts")} && ` +
        `npx ts-node ${path.resolve(__dirname, "deploy-developer.ts")} && ` +
        `npx ts-node ${path.resolve(__dirname, "deploy-stack.ts")} && ` +
        `sleep 10`,
        0,
        true);

    tmuxSession.attach();
})
