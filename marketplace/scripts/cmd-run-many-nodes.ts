import path from "path";
import { argv } from "process";
import { ProcessMultiplexer } from "./process-multiplexer";
import { asyncMain, run, sleep } from "./util";

asyncMain(async () => {
    const numSeeds = parseInt(argv[2]) ?? 3;
    const numTotal = parseInt(argv[3]) ?? 10;

    console.log(`Starting ${numSeeds} seeds and ${numTotal - numSeeds} normal nodes`);

    const executorPath = path.resolve(__dirname, "../../executor");
    const executorManifestPath = path.resolve(executorPath, "Cargo.toml");
    const configFilePath = path.resolve(executorPath, "mu-conf.yaml");
    const devConfigFilePath = path.resolve(executorPath, "mu-conf.dev.yaml");

    run(`env -C ${executorPath} cargo build`);

    let muxer = new ProcessMultiplexer();

    var seedEnvVars = "";

    for (let i = 0; i < numTotal; ++i) {
        seedEnvVars += `MU__INITIAL_CLUSTER[${i}]__ADDRESS=127.0.0.1 ` +
            `MU__INITIAL_CLUSTER[${i}]__GOSSIP_PORT=${20100 + i} ` +
            `MU__INITIAL_CLUSTER[${i}]__PD_PORT=${20200 + i} `;

        let tempDir = `/tmp/mu-executor/${i}/`;
        run(`mkdir -p '${tempDir}' && cp '${configFilePath}' '${tempDir}' && cp '${devConfigFilePath}' '${tempDir}'`);

        let port = 20100 + i;
        let name = i < numSeeds ? `seed-${i + 1}` : `node-${i + 1}`;

        muxer.spawnNew(
            `env -C ${tempDir} ` +
            `MU__CONNECTION_MANAGER__LISTEN_PORT=${port} ` +
            `MU__GATEWAY_MANAGER__LISTEN_PORT=${port} ` +
            seedEnvVars +
            ` cargo run --manifest-path ${executorManifestPath}`,
            name);

	await sleep(5);
    }

    await muxer.waitForAllWithSigint();
});
