import { lightTheme } from "./lightTheme";

it("checks lightTheme's snapshot", () => {
  expect(lightTheme).toMatchSnapshot();
});
