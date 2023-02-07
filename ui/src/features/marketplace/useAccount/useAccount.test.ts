import {renderHook} from "@testing-library/react-hooks";
import {describe, expect} from "vitest";
import useAccount from "@/features/marketplace/useAccount/useAccount";

describe("useAccount", () => {
    test("it should return the result of #getAccountMethod()", async () => {
        const seed = Math.random().toString();
        const promise = Promise.resolve(seed);
        const {result, waitForNextUpdate} = renderHook(() => useAccount(promise, ""));
        await waitForNextUpdate;
        const [resolvedValue] = result.current;

        expect(resolvedValue).toEqual(seed);
    });

    test("it should have #isLoading as #true and after the promise resolves #isLoading should be #false", async () => {
        const seed = Math.random().toString();
        const promise = Promise.resolve(seed);
        const {result, waitForNextUpdate} = renderHook(() => useAccount(promise, ""));
        const [_, beforeResolveIsLoading] = result.current;
        expect(beforeResolveIsLoading).toBeTruthy();
        await waitForNextUpdate;
        await waitForNextUpdate; // we need to call  this twice for the second side effect to happen
        const [value, afterResolveIsLoading] = result.current;
        expect(afterResolveIsLoading).toBeFalsy();
    });
})