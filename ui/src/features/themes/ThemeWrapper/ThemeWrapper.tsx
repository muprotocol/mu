import { ReactNode } from "react";

import { CssBaseline, ThemeProvider } from "@mui/material";

import { lightTheme } from "../lightTheme/lightTheme";

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
