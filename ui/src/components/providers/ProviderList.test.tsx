import {describe, test} from "vitest";
import {render, screen} from "@testing-library/react";
import ProviderList from "@/components/providers/ProviderList";

describe("ProviderList", () => {
    test("it should render", () => {
        render(<ProviderList />)
        screen.debug();
    })
})