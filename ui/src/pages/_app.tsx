import type { AppProps } from "next/app";

import Header from "@/features/layout/Header";
import ThemeWrapper from "@/features/themes/ThemeWrapper/ThemeWrapper";
import WalletWrapper from "@/features/wallet/WalletWrapper/WalletWrapper";
import "@/styles/globals.css";

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