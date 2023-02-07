import type { AppProps } from "next/app";

import Header from "@/features/layout/Header";
import WalletWrapper from "@/features/wallet/WalletWrapper/WalletWrapper";
import "@/styles/globals.css";

import "@/features/theme/PrimeReact.config";

export default function App({ Component, pageProps }: AppProps) {
  return (
    <WalletWrapper>
      <Header />
      <Component {...pageProps} />
    </WalletWrapper>
  );
}
