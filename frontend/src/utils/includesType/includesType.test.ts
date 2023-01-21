import {describe, expect} from "vitest";
import {Buffer, Bytes} from "@keystonehq/bc-ur-registry";
import includesType from "@/utils/includesType/includesType";

describe("includesType", () => {
    test("it should return true if #instanceof target element exists", () => {
        const array: any = [
            new Promise(() => {
            }),
            new Buffer("test")
        ];

        expect(includesType(array, Buffer)).toBeTruthy();
    });

    test("it should return false if #instanceof target element DOES NOT exists", () => {
        const array: any = [
            new Promise(() => {
            }),
        ];

        expect(includesType(array, Buffer)).toBeFalsy();
    })

    test("it should return false if array is empty", () => {
        expect(includesType([], Buffer)).toBeFalsy();
    });
});