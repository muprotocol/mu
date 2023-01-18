import type {AppProps} from 'next/app'
import Header from '@/src/components/Header'
import {ConnectionProvider, WalletProvider} from "@solana/wallet-adapter-react";
import {WalletModalProvider} from '@solana/wallet-adapter-react-ui';

import '@/src/styles/globals.css'
import '@solana/wallet-adapter-react-ui/styles.css'

import endpoint from "@/src/constants/endpoint/endpoint";
import useWallet from '../hooks/useWallet';

export default function App({Component, pageProps}: AppProps) {
    const wallets = useWallet();
    return (
        <>
            <ConnectionProvider endpoint={endpoint()}>
                <WalletProvider wallets={wallets} autoConnect>
                    <WalletModalProvider>
                        <Header/>
                        <Component {...pageProps} />
                    </WalletModalProvider>
                </WalletProvider>
            </ConnectionProvider>
        </>
    )
}
