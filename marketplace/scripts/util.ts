import { spawnSync } from 'child_process';
import { exit, openStdin } from 'process';
import { waitUntilUsed } from 'tcp-port-used';
import { setTimeout } from 'timers/promises';

export const waitUntilPortUsed = (port: number) => waitUntilUsed(port);

export const runAndGetOutput = (command: string): string => {
    let result = spawnSync(command, { shell: true });
    if (result.status !== 0) {
        throw `Command failed with status ${result.status} and output ${result.output}`;
    }
    return result.stdout.toString();
}

export const run = (command: string) => {
    let result = spawnSync(command, { shell: true, stdio: 'inherit' });
    if (result.status !== 0) {
        throw `Command failed with status ${result.status} and output ${result.output}`;
    }
};

export const tryRun = (command: string) => spawnSync(command, { shell: true, stdio: 'inherit' }).status === 0;

export const sleep = (secs: number) => setTimeout(secs * 1000);

export const asyncMain = (f: (() => Promise<number | void>)) =>
    f()
        .then(r => exit(r || 0))
        .catch(e => {
            console.error(e);
            run("sleep 20");
            exit(-1);
        });

export default {
    waitUntilPortUsed,
    runAndGetOutput,
    run,
    tryRun,
    sleep,
    asyncMain
}