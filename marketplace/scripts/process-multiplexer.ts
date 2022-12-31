import {ChildProcessWithoutNullStreams, spawn} from "child_process";
import {sleep} from "./util";

export class ProcessMultiplexer {
    children: [ChildProcessWithoutNullStreams, boolean][] = [];
    maxNameLength = 0;

    public spawnNew(cmd: string, name?: string) {
        name = name || (this.children.length + 1).toString();
        this.maxNameLength = Math.max(this.maxNameLength, name.length);

        let childIndex = this.children.length;
        let child = spawn(cmd, {shell: true});

        let printData = (data: any) => {
            if (typeof (data) === "string") {
                for (let line of data.split('\n')) {
                    line = line.trim();
                    if (line.length > 0) {
                        console.log(`${name.padEnd(this.maxNameLength)}: ${line.trim()}`);
                    }
                }
            } else {
                console.log(`${name.padEnd(this.maxNameLength)}: ${data}`.trim());
            }

        };

        child.stdout.setEncoding('utf8');
        child.stdout.on('data', printData);

        child.stderr.setEncoding('utf8');
        child.stderr.on('data', printData);

        child.on('exit', code => {
            console.log(`Process ${name} exited with code ${code}`);
            this.children[childIndex][1] = true;
        });

        this.children.push([child, false]);
    }

    public async waitForAllToExit() {
        while (!this.children.every(x => x[1])) {
            await sleep(0.1);
        }
    }

    public killAll() {
        for (let child of this.children) {
            if (!child[1]) {
                child[0].kill('SIGKILL');
            }
        }
    }

    public async killAllAndWait() {
        this.killAll();
        await this.waitForAllToExit();
    }

    public async waitForAllWithSigint() {
        let sigintHandler = () => {
            for (let child of this.children) {
                if (!child[1]) {
                    child[0].kill('SIGINT');
                }
            }
        };

        process.on('SIGINT', sigintHandler);

        await this.waitForAllToExit();

        process.off('SIGINT', sigintHandler);
    }
}