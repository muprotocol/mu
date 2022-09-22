import util from "./util"
import { exit } from "process";

util.asyncMain(async () => {
    console.log("Deploying Mu smart contract");

    if (util.tryRun("anchor deploy")) {
        console.log("Mu smart contract deployed");
        await util.sleep(3);
    } else {
        console.log("FAILED TO DEPLOY MU SMART CONTRACT")
        await util.sleep(20);
        return 1;
    }
});
