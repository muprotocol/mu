import { render, screen } from "@testing-library/react";
import { describe, expect, test } from "vitest";

import Home from "./index";
import ProviderList from "./index";

describe("ProviderList", () => {
  test("renders a heading", () => {
    render(<ProviderList />);

    screen.debug()
  });
});
