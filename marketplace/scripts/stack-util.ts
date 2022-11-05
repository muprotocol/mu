import * as base64 from 'base64-js';
import * as fs from "fs";
import path from "path";
import util from "./util";

function ensureStackCliTool() {
    let toolDir = path.resolve(__dirname, ".tools");
    let toolPath = path.join(toolDir, "mu_stack_cli");
    if (fs.existsSync(toolPath))
        return;

    util.run(`env -C ${path.resolve(__dirname, "../../mu_stack")} cargo build --bin mu_stack_cli -r && ` +
        `mkdir ${toolDir} && ` +
        `cp ${path.resolve(__dirname, "../../mu_stack/target/release/mu_stack_cli")} ${toolDir}`);
}

export const yamlToProto = (yamlPath: string): Uint8Array => {
    ensureStackCliTool();

    let b64 = util.runAndGetOutput(`${path.resolve(__dirname, ".tools/mu_stack_cli")} yaml-to-proto -i ${yamlPath}`).trim();
    return base64.toByteArray(b64);
}