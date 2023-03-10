import path from "path";
import { argv } from "process";
import { ProcessMultiplexer } from "./process-multiplexer";
import { asyncMain, run, sleep } from "./util";

asyncMain(async () => {
    const numTotal = parseInt(argv[2]) ?? 10;

    console.log(`Starting ${numTotal} nodes`);

    const executorPath = path.resolve(__dirname, "../../executor");
    const executorManifestPath = path.resolve(executorPath, "Cargo.toml");
    const configFilePath = path.resolve(executorPath, "mu-conf.yaml");
    const devConfigFilePath = path.resolve(executorPath, "mu-conf.dev.yaml");

    run(`env -C ${executorPath} cargo build`);

    let muxer = new ProcessMultiplexer();

    for (let i = 0; i < numTotal; ++i) {
        let tempDir = `/tmp/mu-executor/${i}/`;
        run(`mkdir -p '${tempDir}' && cp '${configFilePath}' '${tempDir}' && cp '${devConfigFilePath}' '${tempDir}'`);

        let port = 20100 + i + 1;
        let name = `node-${i + 1}`;

        muxer.spawnNew(
            `env -C ${tempDir} ` +
            `MU__CONNECTION_MANAGER__LISTEN_PORT=${port} ` +
            `MU__GATEWAY_MANAGER__LISTEN_PORT=${port} ` +
            ` cargo run --manifest-path ${executorManifestPath}`,
            name);

        await sleep(5);
    }

    await muxer.waitForAllWithSigint();
});
