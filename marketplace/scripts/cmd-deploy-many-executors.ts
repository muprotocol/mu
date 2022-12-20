import path from "path";
import { argv } from "process";
import { ProcessMultiplexer } from "./process-multiplexer";
import { asyncMain, run } from "./util";

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

    const seedEnvVars = [...Array(numSeeds).keys()].map(i =>
        `MU__GOSSIP__SEEDS[${i}]__ADDRESS=127.0.0.1 ` +
        `MU__GOSSIP__SEEDS[${i}]__PORT=${20000 + i} `
    ).reduce((x, y) => x + y);

    for (let i = 0; i < numTotal; ++i) {
        let tempDir = `/tmp/mu-executor/${i}/`;
        run(`mkdir -p '${tempDir}' && cp '${configFilePath}' '${tempDir}' && cp '${devConfigFilePath}' '${tempDir}'`);

        let [name, port] = i < numSeeds ? [`seed-${i + 1}`, 20000 + i] : [`node-${i + 1}`, 21000 + i];

        muxer.spawnNew(
            `env -C ${tempDir} ` +
            `MU__CONNECTION_MANAGER__LISTEN_PORT=${port} ` +
            `MU__GATEWAY_MANAGER__LISTEN_PORT=${port} ` +
            seedEnvVars +
            ` cargo run --manifest-path ${executorManifestPath}`,
            name);
    }

    await muxer.waitForAllWithSigint();
});