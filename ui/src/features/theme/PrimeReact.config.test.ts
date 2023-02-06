import { readFileSync } from "fs";
import {describe, expect, test} from "vitest";
import path from "path";

describe("PrimeReact Config", () => {
    test("the snapshot should not have changed", () => {

        expect(readFileSync(`${__dirname}/PrimeReact.config.ts`)).toMatchSnapshot();
    })
});