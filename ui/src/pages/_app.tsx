import type { AppProps } from "next/app";

import Header from "@/features/layout/Header";
import WalletWrapper from "@/features/wallet/WalletWrapper/WalletWrapper";

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
