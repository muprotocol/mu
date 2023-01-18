import type {AppProps} from 'next/app'
import Header from '@/src/components/layout/Header'
import WalletWrapper from "@/src/components/wallet/WalletWrapper";
import '@/src/styles/globals.css'

export default function App({Component, pageProps}: AppProps) {
    return (
        <>
            <WalletWrapper>
                <Header/>
                <Component {...pageProps} />
            </WalletWrapper>
        </>
    )
}
