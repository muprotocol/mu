import { useContext } from "react";
import renderer from "react-test-renderer";

import { render, screen } from "@testing-library/react";

import { ThemeContext } from "@mui/styled-engine";

import ThemeWrapper from "./ThemeWrapper";

describe("ThemeWrapper", () => {
  it("renders ThemeWrapper's children", () => {
    render(
      <ThemeWrapper>
        <div data-testid="children">Children</div>
      </ThemeWrapper>,
    );

    const children = screen.getByTestId("children");
    expect(children).toBeDefined();
  });

  it("provides the mui theme context", () => {
    const Children = () => {
      const themeContext = useContext(ThemeContext);
      expect(themeContext).toBeDefined();
      expect(themeContext).not.toEqual({});

      return <div>Children</div>;
    };

    render(
      <ThemeWrapper>
        <Children />
      </ThemeWrapper>,
    );
  });

  it("checks ThemeWrapper's snapshot", () => {
    const tree = renderer
      .create(
        <ThemeWrapper>
          <div>Children</div>
        </ThemeWrapper>,
      )
      .toJSON();
    expect(tree).toMatchSnapshot();
  });
});
