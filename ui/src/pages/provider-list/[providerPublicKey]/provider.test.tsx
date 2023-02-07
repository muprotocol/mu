import {describe, expect, test} from "vitest";
import {render, screen} from "@testing-library/react";
import Provider from "@/pages/provider-list/[providerPublicKey]/index";

describe("Provider", () => {
    test("it should render", () => {
        render(<Provider params={{providerPublicKey: "test"}} />)
        const providerEl = screen.getByTestId("Provider");

        expect(providerEl).toBeTruthy();
    })
});