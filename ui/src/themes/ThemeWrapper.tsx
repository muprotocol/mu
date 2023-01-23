import { ReactNode } from "react";

import { CssBaseline, ThemeProvider } from "@mui/material";
import { StyledEngineProvider } from "@mui/material/styles";

import { lightTheme } from "./lightTheme";

export type ThemeWrapperProps = {
  children: ReactNode;
};

export default function ThemeWrapper({ children }: ThemeWrapperProps) {
  return (
    <ThemeProvider theme={lightTheme}>
      <CssBaseline />

      {children}
    </ThemeProvider>
  );
}
