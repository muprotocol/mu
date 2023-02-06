import {render, screen} from "@testing-library/react";
import {describe, test} from "vitest";

import ProviderList from "./index";

describe("ProviderList", () => {
  test("renders a heading", () => {
    render(<ProviderList />);

    screen.debug()
  });
});
