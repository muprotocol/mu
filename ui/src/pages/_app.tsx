import type { AppProps } from "next/app";
import Header from "@/components/layout/Header";
import WalletWrapper from "@/components/wallet/WalletWrapper/WalletWrapper";
import "@/styles/globals.css";
import { CssBaseline, ThemeProvider } from "@mui/material";
import { lightTheme } from "../themes/lightTheme";

export default function App({ Component, pageProps }: AppProps) {
  return (
    <ThemeProvider theme={lightTheme}>
      <CssBaseline />

      <WalletWrapper>
        <Header />
        <Component {...pageProps} />
      </WalletWrapper>
    </ThemeProvider>
  );
}
