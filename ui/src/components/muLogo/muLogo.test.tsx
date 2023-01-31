import { render, screen } from "@testing-library/react";
import { beforeEach, describe, expect } from "vitest";

import { MuLogo } from "@/components/muLogo/muLogo";

describe("MuLogo", () => {
  beforeEach(() => {
    render(<MuLogo />);
  });

  it("should render #mu logo svg", () => {
    const muLogoElement = screen.getByTestId("muLogo");

    expect(muLogoElement).toBeTruthy();
  });

  it("should render #protocol logo svg", () => {
    const muLogoElement = screen.getByTestId("protocolLogo");

    expect(muLogoElement).toBeTruthy();
  });
});
