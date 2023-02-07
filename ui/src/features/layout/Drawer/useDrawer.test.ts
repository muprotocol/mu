import {beforeEach, describe, expect} from "vitest";
import {} from "@testing-library/react-hooks";
import useDrawer from "@/features/layout/Drawer/useDrawer";
import {act,renderHook} from "@testing-library/react";

describe("useDrawer", () => {
    it("#isOpen should be initalized with #false", () => {
        const {result} = renderHook(useDrawer);
        const {isOpen} = result.current;

        expect(isOpen).toBeFalsy();
    })

    it("#isOpen should turn to #true on calling #openDrawer()", () => {
        const {result} = renderHook(useDrawer);
        act(() => {
            result.current.openDrawer();
        })
        expect(result.current.isOpen).toBeTruthy();
    })

    it("#isOpen should turn to #false on calling #closeDrawer()", () => {
        const {result} = renderHook(useDrawer);
        act(() => {
            result.current.openDrawer();
        })
        expect(result.current.isOpen).toBeTruthy();
        act(() => {
            result.current.closeDrawer();
        })
        expect(result.current.isOpen).toBeFalsy();
    })
})