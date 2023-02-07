import {describe, expect, test} from "vitest";

import useWithLoading from "@/hooks/useWithLoading/useWithLoading";
import {renderHook} from "@testing-library/react-hooks";

describe("useWithLoading", () => {
    test("it should be true by default", () => {
        const promise = new Promise(() => "test");
        const {result} = renderHook(() => useWithLoading(promise));
        const [isLoading] = result.current;

        expect(isLoading).toBeTruthy();
    });

    test("it should be false on promise resolves", async () => {
        const promise = Promise.resolve("test");
        const {result, waitForNextUpdate} = renderHook(() => useWithLoading(promise));
        await waitForNextUpdate();
        const [isLoading] = result.current;
        expect(isLoading).toBeFalsy();
    });
});
