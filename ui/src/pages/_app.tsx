import type { AppProps } from "next/app";

import Header from "@/shared/layout/Header";
import WalletWrapper from "@/shared/wallet/WalletWrapper/WalletWrapper";

import "@/styles/globals.css";
import ThemeWrapper from "@/themes/ThemeWrapper";

export default function App({ Component, pageProps }: AppProps) {
  return (
    <ThemeWrapper>
      <WalletWrapper>
        <Header />
        <Component {...pageProps} />
      </WalletWrapper>
    </ThemeWrapper>
  );
}
