import path from "path";
import { run } from "./util";

run(`cd ${path.resolve(__dirname, "test-stack")} && npx http-server`);