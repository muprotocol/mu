import util from "./util"

util.asyncMain(async () => {
    console.log("Deploying Mu smart contract");

    if (util.tryRun("anchor deploy")) {
        console.log("Mu smart contract deployed");
    } else {
        console.log("FAILED TO DEPLOY MU SMART CONTRACT")
        return 1;
    }
});
