import { useEffect, useState } from "react";

import { act, render, screen } from "@testing-library/react";
import { beforeEach, describe, expect } from "vitest";

import useDrawer from "@/features/layout/Drawer/useDrawer";

import Drawer from "./Drawer";

describe("Drawer", () => {
  it("should NOT render anything on #isOpen being false", () => {
    render(
      <div data-testid="drawerContainer">
        <Drawer isOpen={false} closeDrawer={() => {}} />
      </div>,
    );
    const drawerContainer = screen.getByTestId("drawerContainer");
    expect(drawerContainer.children.length).toStrictEqual(0);
  });

  it("should render #drawerContent on #isOpen being true", () => {
    render(<Drawer isOpen={true} closeDrawer={() => {}} />);
    const drawerContent = screen.getByTestId("drawerContainer");
    expect(drawerContent).toBeTruthy();
  });
});
