import { spawnSync } from 'child_process';
import { exit } from 'process';
import { waitUntilUsed } from 'tcp-port-used';
import { setTimeout } from 'timers/promises';

const waitUntilPortUsed = (port: number) => waitUntilUsed(port);

const run = (command: string) => {
    let result = spawnSync(command, { shell: true, stdio: 'inherit' });
    if (result.status !== 0) {
        throw `Command failed with status ${result.status} and output ${result.output}`;
    }
};

const tryRun = (command: string) => spawnSync(command, { shell: true, stdio: 'inherit' }).status === 0;

const sleep = (secs: number) => setTimeout(secs * 1000);

const asyncMain = (f: (() => Promise<number | void>)) => (async () => {
    try {
        exit(await f() || 0);
    } catch (e) {
        console.error(e);
        exit(-1);
    }
})();

export default {
    waitUntilPortUsed,
    run,
    tryRun,
    sleep,
    asyncMain
}