import {describe, test, expect} from "vitest";
import endpoint from "./endpoint"

describe("!endpoint", () => {
    test("it should return #NEXT_PUBLIC_API", () => {
        expect(endpoint()).toBe(process.env.VITE_NEXT_PUBLIC_API);
    })
})