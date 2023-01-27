import {describe, expect} from "vitest";
import {renderHook} from "@testing-library/react";
import useProviderList from "@/features/provider/useProviderList/useProviderList";

describe("useProviderList", () => {
    test("it should give a list of providers", () => {
        const {result} = renderHook(() => useProviderList());

        console.log(result.current.providers)

        expect(result.current.providers).toBeDefined();
    })
})