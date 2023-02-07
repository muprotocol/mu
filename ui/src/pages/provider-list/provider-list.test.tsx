import {render, screen} from "@testing-library/react";
import {describe, expect, test} from "vitest";
import mockRouter from 'next-router-mock';
import ProviderList from "./index";

describe("ProviderList", () => {
    vi.mock('next/router', () => require('next-router-mock'));

    test("renders a heading", () => {
        render(<ProviderList/>);
        const providerListEl = screen.getByTestId("ProviderList");

        expect(providerListEl).toBeTruthy();
    });
});
